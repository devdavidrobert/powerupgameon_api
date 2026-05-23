pub mod app_state;
pub mod config;
pub mod controllers;
pub mod error;
pub mod features;
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
    APP.get_or_try_init(|| async {
        init_crypto_providers()?;
        let config = config::Config::load()?;
        let state = AppState::new(config).await?;
        Ok(routes::build_router(state))
    })
    .await
    .map(|router| router.clone())
}

/// Install TLS/JWT crypto providers required by Firestore (gcloud-sdk) before any network I/O.
pub fn init_crypto_providers() -> anyhow::Result<()> {
    use std::sync::OnceLock;
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let _ = jsonwebtoken_v10::crypto::aws_lc::DEFAULT_PROVIDER.install_default();
    });
    Ok(())
}
