use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use cargo_rocket::{app, AppState};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

const ADMIN: &str = "test-admin-token-with-more-than-32-characters";
async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    key: Option<&str>,
    content_type: Option<&str>,
    body: impl Into<Body>,
) -> (StatusCode, Vec<u8>) {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(key) = key {
        request = request.header("Authorization", format!("Bearer {key}"));
    }
    if let Some(mime) = content_type {
        request = request.header("Content-Type", mime);
    }
    let response = app
        .clone()
        .oneshot(request.body(body.into()).unwrap())
        .await
        .unwrap();
    (
        response.status(),
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
}
async fn post(app: &Router, uri: &str, key: &str, body: Value) -> (StatusCode, Value) {
    let (status, body) = call(
        app,
        "POST",
        uri,
        Some(key),
        Some("application/json"),
        body.to_string(),
    )
    .await;
    (status, serde_json::from_slice(&body).unwrap())
}
async fn setup() -> (tempfile::TempDir, Router, String, String) {
    let dir = tempfile::tempdir().unwrap();
    let app = app(AppState::open(
        dir.path().join("db.sqlite").to_str().unwrap(),
        ADMIN,
        "http://localhost:3000",
    )
    .unwrap());
    let (status, backend) = post(&app, "/api/admin/backends", ADMIN, json!({"name":"Local","kind":"fs","options":{"root":dir.path().join("objects").to_str().unwrap()}})).await;
    assert_eq!(status, StatusCode::CREATED, "{backend}");
    let mut keys = Vec::new();
    for name in ["one", "two"] {
        let (status, project) = post(&app, "/api/admin/projects", ADMIN, json!({"name":name,"backend_id":backend["id"],"max_size":32,"allowed_types":["text/plain"]})).await;
        assert_eq!(status, StatusCode::CREATED);
        keys.push(project["api_key"].as_str().unwrap().to_string());
    }
    (dir, app, keys[0].clone(), keys[1].clone())
}
fn upload_path(response: &Value) -> String {
    response["upload_url"]
        .as_str()
        .unwrap()
        .trim_start_matches("http://localhost:3000")
        .to_string()
}
async fn ticket(app: &Router, key: &str, size: u64) -> Value {
    let (status, value) = post(
        app,
        "/api/uploads",
        key,
        json!({"filename":"hello.txt","content_type":"text/plain","size":size}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{value}");
    value
}

#[tokio::test]
async fn upload_download_isolation_and_replay() {
    let (_dir, app, key, other) = setup().await;
    let ticket = ticket(&app, &key, 5).await;
    let path = upload_path(&ticket);
    assert_eq!(
        call(&app, "PUT", &path, None, Some("image/png"), "hello")
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(&app, "PUT", &path, None, Some("text/plain"), "hello")
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "PUT", &path, None, Some("text/plain"), "hello")
            .await
            .0,
        StatusCode::CONFLICT
    );
    let content_path = format!("/api/files/{}/content", ticket["id"].as_str().unwrap());
    let (status, bytes) = call(&app, "GET", &content_path, Some(&key), None, Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"hello");
    assert_eq!(
        call(
            &app,
            "GET",
            &content_path,
            Some(&other),
            None,
            Body::empty()
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&app, "GET", &content_path, None, None, Body::empty())
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn constraints_and_failed_uploads() {
    let (_dir, app, key, _) = setup().await;
    for (filename, mime, size, expected) in [
        ("../escape", "text/plain", 1, StatusCode::BAD_REQUEST),
        ("a", "image/png", 1, StatusCode::BAD_REQUEST),
        ("a", "text/plain", 33, StatusCode::PAYLOAD_TOO_LARGE),
    ] {
        assert_eq!(
            post(
                &app,
                "/api/uploads",
                &key,
                json!({"filename":filename,"content_type":mime,"size":size})
            )
            .await
            .0,
            expected
        );
    }
    for (declared, body, expected) in [
        (2, "long", StatusCode::PAYLOAD_TOO_LARGE),
        (6, "short", StatusCode::BAD_REQUEST),
    ] {
        let ticket = ticket(&app, &key, declared).await;
        let path = upload_path(&ticket);
        assert_eq!(
            call(&app, "PUT", &path, None, Some("text/plain"), body)
                .await
                .0,
            expected
        );
        let (_, metadata) = call(
            &app,
            "GET",
            &format!("/api/files/{}", ticket["id"].as_str().unwrap()),
            Some(&key),
            None,
            Body::empty(),
        )
        .await;
        assert_eq!(
            serde_json::from_slice::<Value>(&metadata).unwrap()["status"],
            "failed"
        );
        assert_eq!(
            call(&app, "PUT", &path, None, Some("text/plain"), body)
                .await
                .0,
            StatusCode::CONFLICT
        );
    }
    let empty = ticket(&app, &key, 0).await;
    assert_eq!(
        call(
            &app,
            "PUT",
            &upload_path(&empty),
            None,
            Some("text/plain"),
            Body::empty()
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn token_expiry_and_concurrent_claims() {
    let (dir, app, key, _) = setup().await;
    let ticket = ticket(&app, &key, 5).await;
    let path = upload_path(&ticket);
    assert_eq!(
        call(
            &app,
            "PUT",
            &format!("{}bad", path),
            None,
            Some("text/plain"),
            "hello"
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (a, b) = tokio::join!(
        call(&app, "PUT", &path, None, Some("text/plain"), "hello"),
        call(&app, "PUT", &path, None, Some("text/plain"), "hello")
    );
    assert!(
        (a.0 == StatusCode::OK && b.0 == StatusCode::CONFLICT)
            || (b.0 == StatusCode::OK && a.0 == StatusCode::CONFLICT)
    );
    let expired = self::ticket(&app, &key, 5).await;
    rusqlite::Connection::open(dir.path().join("db.sqlite"))
        .unwrap()
        .execute(
            "UPDATE uploads SET expires_at=0 WHERE id=?",
            [expired["id"].as_str().unwrap()],
        )
        .unwrap();
    assert_eq!(
        call(
            &app,
            "PUT",
            &upload_path(&expired),
            None,
            Some("text/plain"),
            "hello"
        )
        .await
        .0,
        StatusCode::GONE
    );
}

#[tokio::test]
async fn admin_secrets_and_key_rotation() {
    let (_dir, app, key, _) = setup().await;
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/admin/projects",
            Some(&key),
            None,
            Body::empty()
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        post(
            &app,
            "/api/admin/backends",
            ADMIN,
            json!({"name":"Bad","kind":"ftp","options":{"password":"plaintext"}})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (_, bytes) = call(
        &app,
        "GET",
        "/api/admin/projects",
        Some(ADMIN),
        None,
        Body::empty(),
    )
    .await;
    let projects: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(!String::from_utf8(bytes).unwrap().contains(&key));
    let pid = projects
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "one")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let (status, rotated) = post(
        &app,
        &format!("/api/admin/projects/{pid}/rotate-key"),
        ADMIN,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        post(
            &app,
            "/api/uploads",
            &key,
            json!({"filename":"a","content_type":"text/plain","size":1})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    ticket(&app, rotated["api_key"].as_str().unwrap(), 1).await;
}

#[tokio::test]
async fn streaming_across_chunks_and_restart_persistence() {
    let (dir, app, _key, _) = setup().await;
    let (_, bytes) = call(
        &app,
        "GET",
        "/api/admin/backends",
        Some(ADMIN),
        None,
        Body::empty(),
    )
    .await;
    let backends: Value = serde_json::from_slice(&bytes).unwrap();
    let (_, project) = post(
        &app,
        "/api/admin/projects",
        ADMIN,
        json!({"name":"Streaming", "backend_id":backends[0]["id"], "max_size":20*1024*1024}),
    )
    .await;
    let key = project["api_key"].as_str().unwrap();
    let pending = ticket(&app, key, 9 * 1024 * 1024).await;
    // Reopen SQLite before using the grant: pending URLs and project keys survive restart.
    drop(app);
    let app = self::app(
        AppState::open(
            dir.path().join("db.sqlite").to_str().unwrap(),
            ADMIN,
            "http://localhost:3000",
        )
        .unwrap(),
    );
    let chunks = futures_util::stream::iter(
        (0..144).map(|i| Ok::<_, std::io::Error>(vec![i as u8; 64 * 1024])),
    );
    assert_eq!(
        call(
            &app,
            "PUT",
            &upload_path(&pending),
            None,
            Some("text/plain"),
            Body::from_stream(chunks)
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, bytes) = call(
        &app,
        "GET",
        &format!("/api/files/{}/content", pending["id"].as_str().unwrap()),
        Some(key),
        None,
        Body::empty(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes.len(), 9 * 1024 * 1024);
    for (i, chunk) in bytes.chunks(64 * 1024).enumerate() {
        assert!(chunk.iter().all(|&byte| byte == i as u8));
    }
    let backend_id = backends[0]["id"].as_str().unwrap();
    let (status, result) = post(
        &app,
        &format!("/api/admin/backends/{backend_id}/test"),
        ADMIN,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["ok"], true);
}
