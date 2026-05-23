use axum::{
    body::Body,
    http::Request,
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct RequestContext {
    pub request_id: String,
}

pub async fn request_context_middleware(mut req: Request<Body>, next: Next) -> Response {
    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    req.extensions_mut()
        .insert(RequestContext {
            request_id: request_id.clone(),
        });

    let start = std::time::Instant::now();
    let mut response = next.run(req).await;
    response
        .headers_mut()
        .insert("X-Request-Id", request_id.parse().unwrap());

    let duration_ms = start.elapsed().as_millis();
    let status = response.status().as_u16();
    if status >= 500 {
        tracing::error!(duration_ms, status, "http_request");
    } else {
        tracing::info!(duration_ms, status, "http_request");
    }

    response
}
