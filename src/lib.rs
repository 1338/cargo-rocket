//! Rocket Cargo's HTTP API, storage adapters, and persistent upload grants.
//!
//! The binary provides configuration and process lifecycle; [`app`] assembles
//! the service around an [`AppState`] backed by SQLite.

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use futures_util::StreamExt;
use opendal::{Operator, Scheme};
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tower_http::cors::CorsLayer;
use uuid::Uuid;

type Result<T> = std::result::Result<T, ApiError>;
#[derive(Debug)]
pub struct ApiError(StatusCode, String);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}
impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        tracing::error!(error = %e, "database operation failed");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database operation failed".into(),
        )
    }
}
fn bad(msg: &str) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.into())
}
fn missing() -> ApiError {
    ApiError(StatusCode::NOT_FOUND, "Not found".into())
}
fn storage_error(e: opendal::Error) -> ApiError {
    tracing::error!(error = %e, "storage operation failed");
    ApiError(
        StatusCode::BAD_GATEWAY,
        "Storage operation failed; check server logs and backend configuration".into(),
    )
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
fn id() -> String {
    Uuid::new_v4().to_string()
}
fn secret() -> String {
    let mut bytes = [0; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}
fn hash(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
fn bearer(headers: &HeaderMap) -> Result<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "Bearer token required".into()))
}

/// Shared configuration, database connection, and upload concurrency limiter.
#[derive(Clone)]
pub struct AppState {
    db: Arc<Mutex<Connection>>,
    admin_hash: String,
    base: String,
    slots: Arc<tokio::sync::Semaphore>,
}
impl AppState {
    /// Open the database, initialize its schema, and recover interrupted grants.
    ///
    /// Run only one service instance per database. Storage credentials are
    /// resolved from environment variable references when an adapter is built.
    pub fn open(path: &str, admin: &str, base: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(
            base.starts_with("http://") || base.starts_with("https://"),
            "PUBLIC_BASE_URL must be an HTTP(S) URL"
        );
        let db = Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
          CREATE TABLE IF NOT EXISTS backends (id TEXT PRIMARY KEY, config TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS projects (id TEXT PRIMARY KEY, name TEXT NOT NULL, backend_id TEXT NOT NULL REFERENCES backends(id), key_hash TEXT NOT NULL UNIQUE, max_size INTEGER NOT NULL, allowed_types TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS uploads (id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id), backend_id TEXT NOT NULL REFERENCES backends(id), object_key TEXT NOT NULL UNIQUE, filename TEXT NOT NULL, content_type TEXT NOT NULL, size INTEGER NOT NULL, token_hash TEXT NOT NULL, expires_at INTEGER NOT NULL, status TEXT NOT NULL, created_at INTEGER NOT NULL);
          CREATE INDEX IF NOT EXISTS uploads_project ON uploads(project_id, created_at);
          UPDATE uploads SET status='failed' WHERE status='uploading';")?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
            admin_hash: hash(admin),
            base: base.trim_end_matches('/').into(),
            slots: Arc::new(tokio::sync::Semaphore::new(16)),
        })
    }
    fn admin(&self, headers: &HeaderMap) -> Result<()> {
        if hash(bearer(headers)?) != self.admin_hash {
            return Err(ApiError(
                StatusCode::UNAUTHORIZED,
                "Invalid admin token".into(),
            ));
        }
        Ok(())
    }
    fn project(&self, headers: &HeaderMap) -> Result<Project> {
        self.db
            .lock()
            .unwrap()
            .query_row(
                "SELECT id,name,backend_id,max_size,allowed_types FROM projects WHERE key_hash=?",
                [hash(bearer(headers)?)],
                project_row,
            )
            .optional()?
            .ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "Invalid project API key".into()))
    }
    fn backend(&self, id: &str) -> Result<Backend> {
        let config: String = self
            .db
            .lock()
            .unwrap()
            .query_row("SELECT config FROM backends WHERE id=?", [id], |r| r.get(0))
            .optional()?
            .ok_or_else(missing)?;
        serde_json::from_str(&config).map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Invalid stored backend configuration".into(),
            )
        })
    }
}

/// Build the API and embedded dashboard, with CORS and response security headers.
pub fn app(state: AppState) -> Router {
    let mut cors = CorsLayer::new()
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);
    if let Ok(origins) = std::env::var("CORS_ORIGINS") {
        let origins: Vec<_> = origins
            .split(',')
            .filter_map(|o| o.trim().parse::<axum::http::HeaderValue>().ok())
            .collect();
        cors = cors.allow_origin(origins);
    }
    Router::new()
        .route("/", get(|| async { Html(include_str!("../static/index.html")) }))
        .route("/dashboard.js", get(|| async { ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], include_str!("../static/dashboard.js")) }))
        .route("/health", get(|| async { Json(json!({"status":"ok"})) }))
        .route("/api/admin/backends", get(list_backends).post(create_backend))
        .route("/api/admin/backends/{id}/test", post(test_backend))
        .route("/api/admin/projects", get(list_projects).post(create_project))
        .route("/api/admin/projects/{id}/rotate-key", post(rotate_key))
        .route("/api/admin/uploads", get(list_uploads))
        .route("/api/uploads", post(request_upload))
        .route("/api/files/{id}", get(file_metadata))
        .route("/api/files/{id}/content", get(download))
        .route("/upload/{id}", put(upload).layer(DefaultBodyLimit::disable()))
        .layer(DefaultBodyLimit::max(32 * 1024))
        .layer(cors)
        .layer(axum::middleware::from_fn(|req: axum::extract::Request, next: axum::middleware::Next| async move {
            let mut response = next.run(req).await;
            let headers = response.headers_mut();
            headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
            headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
            headers.insert("referrer-policy", "no-referrer".parse().unwrap());
            headers.insert(header::CONTENT_SECURITY_POLICY, "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'".parse().unwrap());
            response
        }))
        .with_state(state)
}

#[derive(Clone, Serialize, Deserialize)]
struct Backend {
    #[serde(default)]
    id: String,
    name: String,
    kind: String,
    #[serde(default)]
    options: HashMap<String, String>,
    #[serde(default)]
    secrets: HashMap<String, String>,
}
impl Backend {
    fn operator(&self) -> Result<Operator> {
        let scheme = match self.kind.as_str() {
            "fs" => Scheme::Fs,
            "s3" => Scheme::S3,
            "azblob" => Scheme::Azblob,
            "ftp" => Scheme::Ftp,
            "sftp" => Scheme::Sftp,
            _ => return Err(bad("Unsupported backend kind")),
        };
        let mut options = self.options.clone();
        for (key, env) in &self.secrets {
            let value = std::env::var(env)
                .map_err(|_| bad(&format!("Required environment variable {env} is not set")))?;
            options.insert(key.clone(), value);
        }
        Operator::via_iter(scheme, options).map_err(storage_error)
    }
    fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() || self.name.len() > 100 {
            return Err(bad("Backend name must be 1–100 characters"));
        }
        let allowed: &[&str] = match self.kind.as_str() {
            "fs" => &["root"],
            "s3" => &[
                "root",
                "bucket",
                "region",
                "endpoint",
                "access_key_id",
                "secret_access_key",
                "session_token",
                "enable_virtual_host_style",
            ],
            "azblob" => &[
                "root",
                "container",
                "endpoint",
                "account_name",
                "account_key",
                "sas_token",
            ],
            "ftp" => &["root", "endpoint", "user", "password"],
            "sftp" => &["root", "endpoint", "user", "key", "known_hosts_strategy"],
            _ => return Err(bad("Choose fs, s3, azblob, ftp, or sftp")),
        };
        for key in self.options.keys().chain(self.secrets.keys()) {
            if !allowed.contains(&key.as_str()) {
                return Err(bad(&format!("Unsupported option: {key}")));
            }
        }
        for key in [
            "secret_access_key",
            "session_token",
            "account_key",
            "sas_token",
            "password",
            "access_key_id",
        ] {
            if self.options.contains_key(key) {
                return Err(bad(
                    "Credentials must be environment variable references in secrets",
                ));
            }
        }
        for env in self.secrets.values() {
            if env.is_empty() || !env.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
                return Err(bad("Secret references must be environment variable names"));
            }
        }
        if self.kind == "sftp"
            && self
                .options
                .get("known_hosts_strategy")
                .is_some_and(|v| v != "Strict")
        {
            return Err(bad(
                "SFTP requires Strict host verification; provision known_hosts first",
            ));
        }
        if self.kind == "fs"
            && !self
                .options
                .get("root")
                .is_some_and(|v| std::path::Path::new(v).is_absolute())
        {
            return Err(bad("Local storage requires an absolute root path"));
        }
        Ok(())
    }
}
async fn create_backend(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(mut b): Json<Backend>,
) -> Result<impl IntoResponse> {
    s.admin(&headers)?;
    b.validate()?;
    b.operator()?;
    b.id = id();
    s.db.lock().unwrap().execute(
        "INSERT INTO backends VALUES (?,?)",
        params![b.id, serde_json::to_string(&b).unwrap()],
    )?;
    Ok((StatusCode::CREATED, Json(b)))
}
async fn list_backends(State(s): State<AppState>, headers: HeaderMap) -> Result<Json<Value>> {
    s.admin(&headers)?;
    let db = s.db.lock().unwrap();
    let mut q = db.prepare("SELECT config FROM backends ORDER BY rowid DESC")?;
    let rows = q
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(Json(json!(rows
        .into_iter()
        .map(|r| serde_json::from_str::<Value>(&r).unwrap())
        .collect::<Vec<_>>())))
}
async fn test_backend(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    s.admin(&headers)?;
    let op = s.backend(&id)?.operator()?;
    let key = format!(".rocket-cargo-probe/{}", self::id());
    let result = tokio::time::timeout(Duration::from_secs(20), async {
        op.write(&key, "rocket-cargo connection test").await?;
        let data = op.read(&key).await?;
        if data.to_bytes().as_ref() != b"rocket-cargo connection test" {
            return Err(opendal::Error::new(
                opendal::ErrorKind::Unexpected,
                "Read-back mismatch",
            ));
        }
        Ok::<_, opendal::Error>(())
    })
    .await;
    let cleanup = tokio::time::timeout(Duration::from_secs(10), op.delete(&key)).await;
    result
        .map_err(|_| ApiError(StatusCode::GATEWAY_TIMEOUT, "Storage test timed out".into()))?
        .map_err(storage_error)?;
    cleanup
        .map_err(|_| {
            ApiError(
                StatusCode::GATEWAY_TIMEOUT,
                "Probe cleanup timed out".into(),
            )
        })?
        .map_err(storage_error)?;
    Ok(Json(json!({"ok":true})))
}

#[derive(Serialize)]
struct Project {
    id: String,
    name: String,
    backend_id: String,
    max_size: u64,
    allowed_types: Vec<String>,
}
fn project_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: r.get(0)?,
        name: r.get(1)?,
        backend_id: r.get(2)?,
        max_size: r.get(3)?,
        allowed_types: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default(),
    })
}
#[derive(Deserialize)]
struct NewProject {
    name: String,
    backend_id: String,
    #[serde(default = "default_size")]
    max_size: u64,
    #[serde(default)]
    allowed_types: Vec<String>,
}
fn default_size() -> u64 {
    100 * 1024 * 1024
}
async fn create_project(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(p): Json<NewProject>,
) -> Result<impl IntoResponse> {
    s.admin(&headers)?;
    if p.name.trim().is_empty()
        || p.name.len() > 100
        || p.max_size == 0
        || p.max_size > 10 * 1024 * 1024 * 1024
    {
        return Err(bad("Name required; max_size must be 1 byte to 10 GiB"));
    }
    if p.allowed_types.iter().any(|t| !valid_mime(t)) {
        return Err(bad(
            "Allowed types must be exact MIME types, e.g. image/png",
        ));
    }
    s.backend(&p.backend_id)?;
    let key = secret();
    let pid = id();
    s.db.lock().unwrap().execute(
        "INSERT INTO projects VALUES (?,?,?,?,?,?)",
        params![
            pid,
            p.name,
            p.backend_id,
            hash(&key),
            p.max_size,
            serde_json::to_string(&p.allowed_types).unwrap()
        ],
    )?;
    Ok((StatusCode::CREATED, Json(json!({"id":pid,"api_key":key}))))
}
async fn list_projects(State(s): State<AppState>, headers: HeaderMap) -> Result<Json<Value>> {
    s.admin(&headers)?;
    let db = s.db.lock().unwrap();
    let mut q = db.prepare(
        "SELECT id,name,backend_id,max_size,allowed_types FROM projects ORDER BY rowid DESC",
    )?;
    let rows = q
        .query_map([], project_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(Json(json!(rows)))
}
async fn rotate_key(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    s.admin(&headers)?;
    let key = secret();
    if s.db.lock().unwrap().execute(
        "UPDATE projects SET key_hash=? WHERE id=?",
        params![hash(&key), id],
    )? == 0
    {
        return Err(missing());
    }
    Ok(Json(json!({"api_key":key})))
}

#[derive(Deserialize)]
struct UploadRequest {
    filename: String,
    content_type: String,
    size: u64,
}
#[derive(Serialize)]
struct File {
    id: String,
    project_id: String,
    backend_id: String,
    key: String,
    filename: String,
    content_type: String,
    size: u64,
    expires_at: i64,
    status: String,
    created_at: i64,
}
fn file_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<File> {
    Ok(File {
        id: r.get(0)?,
        project_id: r.get(1)?,
        backend_id: r.get(2)?,
        key: r.get(3)?,
        filename: r.get(4)?,
        content_type: r.get(5)?,
        size: r.get(6)?,
        expires_at: r.get(8)?,
        status: r.get(9)?,
        created_at: r.get(10)?,
    })
}
fn valid_mime(s: &str) -> bool {
    s.len() <= 128
        && s.split('/').count() == 2
        && s.split('/').all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"!#$&^_.+-".contains(&c))
        })
}
async fn request_upload(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(r): Json<UploadRequest>,
) -> Result<impl IntoResponse> {
    let p = s.project(&headers)?;
    if r.size > p.max_size {
        return Err(ApiError(
            StatusCode::PAYLOAD_TOO_LARGE,
            "File exceeds project size limit".into(),
        ));
    }
    if r.filename.is_empty()
        || r.filename.len() > 255
        || r.filename
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
    {
        return Err(bad(
            "Filename must be 1–255 bytes without path separators or control characters",
        ));
    }
    if !valid_mime(&r.content_type)
        || (!p.allowed_types.is_empty() && !p.allowed_types.contains(&r.content_type))
    {
        return Err(bad(
            "Content type is invalid or not allowed for this project",
        ));
    }
    let uid = id();
    let token = secret();
    let expires = now() + 900;
    let key = format!("{}/{}", p.id, uid);
    s.db.lock().unwrap().execute(
        "INSERT INTO uploads VALUES (?,?,?,?,?,?,?,?,?,'pending',?)",
        params![
            uid,
            p.id,
            p.backend_id,
            key,
            r.filename,
            r.content_type,
            r.size,
            hash(&token),
            expires,
            now()
        ],
    )?;
    Ok((
        StatusCode::CREATED,
        Json(
            json!({"id":uid,"key":key,"upload_url":format!("{}/upload/{}?token={}",s.base,uid,token),"method":"PUT","headers":{"Content-Type":r.content_type},"expires_at":expires,"file_url":format!("{}/api/files/{}/content",s.base,uid)}),
        ),
    ))
}
#[derive(Deserialize)]
struct UploadQuery {
    token: String,
}
async fn upload(
    State(s): State<AppState>,
    Path(uid): Path<String>,
    Query(q): Query<UploadQuery>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<Value>> {
    let _permit = s.slots.clone().try_acquire_owned().map_err(|_| {
        ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "Upload capacity reached; retry shortly".into(),
        )
    })?;
    let file = {
        let db = s.db.lock().unwrap();
        let (file, expected): (File, String) = db
            .query_row("SELECT * FROM uploads WHERE id=?", [&uid], |r| {
                Ok((file_row(r)?, r.get(7)?))
            })
            .optional()?
            .ok_or_else(missing)?;
        if hash(&q.token) != expected {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                "Invalid upload token".into(),
            ));
        }
        if file.expires_at <= now() {
            return Err(ApiError(StatusCode::GONE, "Upload URL expired".into()));
        }
        if file.status != "pending" {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "Upload URL already used".into(),
            ));
        }
        if headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            != Some(file.content_type.as_str())
        {
            return Err(bad("Content-Type must match the upload request exactly"));
        }
        if let Some(length) = headers.get(header::CONTENT_LENGTH) {
            if length.to_str().ok().and_then(|v| v.parse::<u64>().ok()) != Some(file.size) {
                return Err(bad("Content-Length must match the declared size"));
            }
        }
        file
    };
    let op = s.backend(&file.backend_id)?.operator()?;
    if s.db.lock().unwrap().execute(
        "UPDATE uploads SET status='uploading' WHERE id=? AND status='pending' AND expires_at>?",
        params![uid, now()],
    )? != 1
    {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Upload URL already used or expired".into(),
        ));
    }
    let mut guard = UploadGuard {
        state: s.clone(),
        id: uid.clone(),
        op: op.clone(),
        key: file.key.clone(),
        complete: false,
    };
    let result = tokio::time::timeout(Duration::from_secs(300), async {
        let mut writer = op
            .writer_with(&file.key)
            .chunk(8 * 1024 * 1024)
            .await
            .map_err(storage_error)?;
        let mut stream = body.into_data_stream();
        let mut received = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| bad("Upload body interrupted"))?;
            received += chunk.len() as u64;
            if received > file.size {
                let _ = writer.abort().await;
                return Err(ApiError(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "Body exceeds declared size".into(),
                ));
            }
            writer.write(chunk).await.map_err(storage_error)?;
        }
        if received != file.size {
            let _ = writer.abort().await;
            return Err(bad("Body does not match declared size"));
        }
        writer.close().await.map_err(storage_error)?;
        Ok(())
    })
    .await
    .map_err(|_| {
        ApiError(
            StatusCode::REQUEST_TIMEOUT,
            "Upload timed out; request a new upload URL".into(),
        )
    })?;
    result?;
    s.db.lock()
        .unwrap()
        .execute("UPDATE uploads SET status='complete' WHERE id=?", [&uid])?;
    guard.complete = true;
    Ok(Json(json!({"id":uid,"key":file.key,"status":"complete"})))
}
struct UploadGuard {
    state: AppState,
    id: String,
    op: Operator,
    key: String,
    complete: bool,
}
impl Drop for UploadGuard {
    fn drop(&mut self) {
        if !self.complete {
            let _ = self
                .state
                .db
                .lock()
                .unwrap()
                .execute("UPDATE uploads SET status='failed' WHERE id=?", [&self.id]);
            let op = self.op.clone();
            let key = self.key.clone();
            tokio::spawn(async move {
                let _ = tokio::time::timeout(Duration::from_secs(15), op.delete(&key)).await;
            });
        }
    }
}
fn owned_file(s: &AppState, headers: &HeaderMap, uid: &str) -> Result<File> {
    let p = s.project(headers)?;
    let mut f =
        s.db.lock()
            .unwrap()
            .query_row(
                "SELECT * FROM uploads WHERE id=? AND project_id=?",
                params![uid, p.id],
                file_row,
            )
            .optional()?
            .ok_or_else(missing)?;
    if f.status == "pending" && f.expires_at <= now() {
        f.status = "expired".into();
    }
    Ok(f)
}
async fn file_metadata(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(uid): Path<String>,
) -> Result<Json<File>> {
    Ok(Json(owned_file(&s, &headers, &uid)?))
}
async fn download(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(uid): Path<String>,
) -> Result<Response> {
    let f = owned_file(&s, &headers, &uid)?;
    if f.status != "complete" {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "File is not complete".into(),
        ));
    }
    let reader = s
        .backend(&f.backend_id)?
        .operator()?
        .reader(&f.key)
        .await
        .map_err(storage_error)?;
    let stream = reader.into_bytes_stream(..).await.map_err(storage_error)?;
    // Force attachment and a generic type: untrusted uploads must not execute on the dashboard origin.
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", f.id),
            ),
            (header::CONTENT_LENGTH, f.size.to_string()),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}
async fn list_uploads(State(s): State<AppState>, headers: HeaderMap) -> Result<Json<Value>> {
    s.admin(&headers)?;
    let db = s.db.lock().unwrap();
    let mut q =
        db.prepare("SELECT * FROM uploads ORDER BY created_at DESC, rowid DESC LIMIT 100")?;
    let mut rows = q
        .query_map([], file_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for f in &mut rows {
        if f.status == "pending" && f.expires_at <= now() {
            f.status = "expired".into();
        }
    }
    Ok(Json(json!(rows)))
}
