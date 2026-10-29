mod config;

use cargo_rocket::{app, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let token = std::env::var("ADMIN_TOKEN").map_err(|_| {
        anyhow::anyhow!("Set ADMIN_TOKEN to a random secret of at least 32 characters")
    })?;
    anyhow::ensure!(
        token.len() >= 32,
        "ADMIN_TOKEN must be at least 32 characters"
    );
    let config = config::ServerConfig::from_env()?;
    let db = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "data/upload-tool.db".into());
    if let Some(parent) = std::path::Path::new(&db)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let state = AppState::open(&db, &token, &config.public_base_url)?;
    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!(address = %listener.local_addr()?, url = %config.public_base_url, "cargo-rocket listening");
    axum::serve(listener, app(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
