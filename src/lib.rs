pub mod app_state;
pub mod config;
pub mod controllers;
pub mod error;
pub mod logger;
pub mod middleware;
pub mod models;
pub mod routes;
pub mod services;
pub mod utils;

use app_state::AppState;
use axum::Router;
use tokio::sync::OnceCell;

static APP: OnceCell<Router> = OnceCell::const_new();

/// Build the Axum router once per serverless instance (or per long-running process).
pub async fn build_app() -> anyhow::Result<Router> {
    if let Some(router) = APP.get() {
        return Ok(router.clone());
    }

    init_crypto_providers()?;
    let config = config::Config::load()?;
    let state = AppState::new(config).await?;
    let router = routes::build_router(state);
    let _ = APP.set(router.clone());
    Ok(router)
}

/// Install TLS/JWT crypto providers required by Firestore (gcloud-sdk) before any network I/O.
pub fn init_crypto_providers() -> anyhow::Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("Failed to install rustls crypto provider"))?;
    jsonwebtoken_v10::crypto::aws_lc::DEFAULT_PROVIDER
        .install_default()
        .map_err(|_| anyhow::anyhow!("Failed to install jsonwebtoken crypto provider"))?;
    Ok(())
}
