use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware,
    routing::get,
    Router,
};
use powerupgameon_api::middleware::request_context::{
    request_context_middleware, resolve_request_id,
};
use tower::ServiceExt;

#[test]
fn resolve_request_id_uses_valid_client_value() {
    assert_eq!(
        resolve_request_id(Some("client-trace-abc123")),
        "client-trace-abc123"
    );
}

#[test]
fn resolve_request_id_rejects_empty_client_value() {
    let id = resolve_request_id(Some(""));
    assert!(!id.is_empty());
    assert_ne!(id, "");
}

#[test]
fn resolve_request_id_generates_uuid_when_header_missing() {
    let id = resolve_request_id(None);
    assert!(!id.is_empty());
    assert_ne!(id, "");
}

#[tokio::test]
async fn request_context_middleware_echoes_valid_request_id() {
    let app = Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(middleware::from_fn(request_context_middleware));

    let response = app
        .oneshot(
            Request::builder()
                .header("x-request-id", "trace-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("X-Request-Id").unwrap(), "trace-123");
}

#[tokio::test]
async fn request_context_middleware_does_not_panic_on_invalid_request_id() {
    let app = Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(middleware::from_fn(request_context_middleware));

    let response = app
        .oneshot(
            Request::builder()
                .header("x-request-id", "")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let echoed = response.headers().get("X-Request-Id").unwrap();
    assert!(!echoed.to_str().unwrap().is_empty());
}
