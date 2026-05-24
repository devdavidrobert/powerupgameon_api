//! Ensures Vercel rewrite simulation (`/api/main?__path=...`) reaches every route group.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::from_fn,
    routing::{delete, get, post, put},
    Router,
};
use powerupgameon_api::middleware::vercel_path::restore_vercel_path;
use tower::{Layer, Service, ServiceExt};

fn vercel_routes() -> Router {
    let campaign_slug_routes = Router::new()
        .route("/settings", put(|| async { "settings" }))
        .route("/questions", get(|| async { "questions" }))
        .route("/questions/admin/full", get(|| async { "questions-admin" }))
        .route("/prizes", get(|| async { "prizes" }))
        .route("/registrations", post(|| async { "registrations" }))
        .route("/submissions", post(|| async { "submissions" }))
        .route("/spin", post(|| async { "spin" }))
        .route("/locations", get(|| async { "locations" }))
        .route("/inventory", get(|| async { "inventory" }))
        .route("/raffles", get(|| async { "raffles" }))
        .route(
            "/raffles/{raffle_id}/winners",
            get(|| async { "raffle-winners" }),
        )
        .route(
            "/raffles/winners/{winner_id}",
            axum::routing::patch(|| async { "raffle-winner-patch" }),
        )
        .route("/settings/timers", delete(|| async { "timers" }));

    Router::new()
        .route("/health", get(|| async { "health" }))
        .route("/api/csrf-token", get(|| async { "csrf" }))
        .route(
            "/api/campaigns",
            get(|| async { "campaigns-list" }).post(|| async { "campaigns-create" }),
        )
        .route(
            "/api/campaigns/{slug}",
            get(|| async { "campaign-by-slug" })
                .put(|| async { "campaign-update" })
                .delete(|| async { "campaign-archive" }),
        )
        .nest("/api/campaigns/{slug}", campaign_slug_routes)
        .route("/api/auth/verify", post(|| async { "auth-verify" }))
        .route("/api/auth/session", post(|| async { "auth-session" }))
}

fn vercel_app(
) -> impl Service<Request<Body>, Response = axum::response::Response, Error = std::convert::Infallible>
       + Clone {
    from_fn(restore_vercel_path).layer(vercel_routes())
}

async fn assert_vercel_route(
    mut app: impl Service<
        Request<Body>,
        Response = axum::response::Response,
        Error = std::convert::Infallible,
    >,
    method: &str,
    original_path: &str,
) {
    let response = app
        .call(
            Request::builder()
                .method(method)
                .uri(format!("/api/main?__path={original_path}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{method} {original_path} should match after path restore"
    );
}

#[tokio::test]
async fn vercel_restore_reaches_all_route_groups() {
    let app = vercel_app();

    assert_vercel_route(app.clone(), "GET", "health").await;
    assert_vercel_route(app.clone(), "GET", "api/csrf-token").await;
    assert_vercel_route(app.clone(), "GET", "api/campaigns").await;
    assert_vercel_route(app.clone(), "POST", "api/campaigns").await;
    assert_vercel_route(app.clone(), "GET", "api/campaigns/test3").await;
    assert_vercel_route(app.clone(), "PUT", "api/campaigns/test3").await;
    assert_vercel_route(app.clone(), "DELETE", "api/campaigns/test3").await;
    assert_vercel_route(app.clone(), "POST", "api/auth/verify").await;
    assert_vercel_route(app.clone(), "POST", "api/auth/session").await;

    assert_vercel_route(app.clone(), "PUT", "api/campaigns/test3/settings").await;
    assert_vercel_route(app.clone(), "GET", "api/campaigns/test3/questions").await;
    assert_vercel_route(
        app.clone(),
        "GET",
        "api/campaigns/test3/questions/admin/full",
    )
    .await;
    assert_vercel_route(app.clone(), "GET", "api/campaigns/test3/prizes").await;
    assert_vercel_route(app.clone(), "POST", "api/campaigns/test3/registrations").await;
    assert_vercel_route(app.clone(), "POST", "api/campaigns/test3/submissions").await;
    assert_vercel_route(app.clone(), "POST", "api/campaigns/test3/spin").await;
    assert_vercel_route(app.clone(), "GET", "api/campaigns/test3/locations").await;
    assert_vercel_route(app.clone(), "GET", "api/campaigns/test3/inventory").await;
    assert_vercel_route(app.clone(), "GET", "api/campaigns/test3/raffles").await;
    assert_vercel_route(app.clone(), "GET", "api/campaigns/test3/raffles/r1/winners").await;
    assert_vercel_route(
        app.clone(),
        "PATCH",
        "api/campaigns/test3/raffles/winners/w1",
    )
    .await;
    assert_vercel_route(app, "DELETE", "api/campaigns/test3/settings/timers").await;
}

#[tokio::test]
async fn vercel_path_without_restore_does_not_match_nested_routes() {
    let app = Router::new()
        .nest(
            "/api/campaigns/{slug}",
            Router::new().route("/settings", put(|| async { "settings" })),
        )
        .fallback(|| async { (StatusCode::NOT_FOUND, "missing") });

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/main?__path=api/campaigns/test3/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
