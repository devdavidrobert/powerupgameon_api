use axum::middleware::from_fn;
use powerupgameon_api::build_app;
use powerupgameon_api::middleware::vercel_path::restore_vercel_path;
use tower::ServiceBuilder;
use vercel_runtime::axum::VercelLayer;
use vercel_runtime::{run, Error};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let router = build_app()
        .await
        .map_err(|err| Error::from(format!("failed to build app: {err}")))?;

    // Must run before Axum route matching — `Router::layer` runs after routing.
    let app = ServiceBuilder::new()
        .layer(VercelLayer::new())
        .layer(from_fn(restore_vercel_path))
        .service(router);

    run(app).await
}
