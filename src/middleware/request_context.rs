use axum::{
    body::Body,
    http::{HeaderValue, Request},
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct RequestContext {
    pub request_id: String,
}

/// Accept a client-provided request ID only when it is safe to echo as a response header.
pub fn resolve_request_id(incoming: Option<&str>) -> String {
    incoming
        .filter(|s| !s.is_empty())
        .and_then(|s| HeaderValue::from_str(s).ok().map(|_| s.to_string()))
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

pub async fn request_context_middleware(mut req: Request<Body>, next: Next) -> Response {
    let request_id = resolve_request_id(
        req.headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok()),
    );

    req.extensions_mut()
        .insert(RequestContext {
            request_id: request_id.clone(),
        });

    let start = std::time::Instant::now();
    let mut response = next.run(req).await;
    if let Ok(header_value) = HeaderValue::from_str(&request_id) {
        response
            .headers_mut()
            .insert("X-Request-Id", header_value);
    }

    let duration_ms = start.elapsed().as_millis();
    let status = response.status().as_u16();
    if status >= 500 {
        tracing::error!(duration_ms, status, "http_request");
    } else {
        tracing::info!(duration_ms, status, "http_request");
    }

    response
}
