use powerupgameon_api::{app_state::AppState, config::Config, init_crypto_providers, routes};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_crypto_providers()?;

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            "powerupgameon_api=info,tower_http=info".into()
        }))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::load()?;
    let state = AppState::new(config.clone()).await?;
    let app = routes::build_router(state.clone());

    let addr = format!("0.0.0.0:{}", config.port);
    tracing::info!(port = config.port, env = %config.node_env, "api_listen");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
