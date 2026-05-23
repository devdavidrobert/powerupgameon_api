use powerupgameon_api::build_app;
use tower::ServiceBuilder;
use vercel_runtime::axum::VercelLayer;
use vercel_runtime::{run, Error};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let router = build_app()
        .await
        .map_err(|err| Error::from(format!("failed to build app: {err}")))?;

    let app = ServiceBuilder::new()
        .layer(VercelLayer::new())
        .service(router);

    run(app).await
}
