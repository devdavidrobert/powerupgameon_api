use axum::extract::ConnectInfo;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::HeaderMap;
use std::net::SocketAddr;

/// Optional TCP peer address from request extensions (standalone server only).
pub struct ClientPeer(pub Option<SocketAddr>);

impl<S> FromRequestParts<S> for ClientPeer
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(ClientPeer(peer_from_extensions(&parts.extensions)))
    }
}

/// Resolve the client IP from proxy headers and/or the TCP peer address.
///
/// When `trust_proxy` is true and `X-Forwarded-For` is present, the leftmost
/// entry is used (correct when the immediate hop is a trusted proxy such as Vercel).
/// Otherwise the socket peer address is preferred; `"unknown"` only when neither is available.
pub fn get_client_ip(
    headers: &HeaderMap,
    trust_proxy: bool,
    peer_addr: Option<SocketAddr>,
) -> String {
    if trust_proxy {
        if let Some(forwarded) = headers.get("x-forwarded-for") {
            if let Ok(value) = forwarded.to_str() {
                if let Some(first) = value.split(',').next() {
                    let trimmed = first.trim();
                    if !trimmed.is_empty() {
                        return trimmed.to_string();
                    }
                }
            }
        }
    }

    if let Some(addr) = peer_addr {
        return addr.ip().to_string();
    }

    "unknown".into()
}

/// Extract peer address from request extensions (set by `into_make_service_with_connect_info`).
pub fn peer_from_extensions(extensions: &http::Extensions) -> Option<SocketAddr> {
    extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| *addr)
}

/// Convenience for middleware: resolve IP from headers + request extensions.
pub fn get_client_ip_from_request(
    headers: &HeaderMap,
    extensions: &http::Extensions,
    trust_proxy: bool,
) -> String {
    get_client_ip(headers, trust_proxy, peer_from_extensions(extensions))
}
