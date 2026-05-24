//! Campaign routing layout used by `routes.rs`:
//! - Admin list/CRUD at explicit `/api/campaigns` paths (merged, not prefix-nested)
//! - Slug sub-resources under `/api/campaigns/{slug}/…`

use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::get,
    Router,
};
use tower::ServiceExt;

fn production_style_campaign_routes() -> Router {
    let admin = Router::new()
        .route("/api/campaigns", get(|| async { "list" }))
        .route("/api/campaigns/{slug}", get(|| async { "slug" }));

    let slug_resources = Router::new()
        .route("/questions", get(|| async { "questions" }))
        .route("/settings", get(|| async { "settings" }));

    Router::new()
        .merge(admin)
        .nest("/api/campaigns/{slug}", slug_resources)
}

#[tokio::test]
async fn slug_subresources_are_reachable() {
    let app = production_style_campaign_routes();

    for uri in ["/api/campaigns/test/questions", "/api/campaigns/test/settings"] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{uri}");
    }
}

#[tokio::test]
async fn campaign_list_and_slug_crud_are_reachable() {
    let app = production_style_campaign_routes();

    for uri in ["/api/campaigns", "/api/campaigns/test"] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{uri}");
    }
}
