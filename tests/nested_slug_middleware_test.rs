use axum::{
    body::Body,
    extract::{Path, Request},
    http::{Request as HttpRequest, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::put,
    Json, Router,
};
use tower::ServiceExt;

#[derive(serde::Deserialize)]
struct SlugPath {
    slug: String,
}

async fn inject_slug(
    Path(slug_path): Path<SlugPath>,
    mut req: Request,
    next: Next,
) -> Response {
    req.extensions_mut().insert(slug_path.slug);
    next.run(req).await
}

async fn handler(req: Request) -> Json<String> {
    let slug = req
        .extensions()
        .get::<String>()
        .cloned()
        .unwrap_or_else(|| "missing".into());
    Json(slug)
}

#[tokio::test]
async fn nested_slug_is_available_to_inner_middleware() {
    let inner = Router::new()
        .route("/settings", put(handler))
        .layer(middleware::from_fn(inject_slug));

    let app = Router::new().nest("/api/campaigns/{slug}", inner);

    let response = app
        .oneshot(
            HttpRequest::builder()
                .method("PUT")
                .uri("/api/campaigns/test2/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, "test2");
}
