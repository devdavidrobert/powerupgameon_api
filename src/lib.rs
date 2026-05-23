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
