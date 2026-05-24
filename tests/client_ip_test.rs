mod common;

use axum::http::HeaderMap;
use powerupgameon_api::utils::client_ip::get_client_ip;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

#[test]
fn uses_socket_peer_when_proxy_not_trusted() {
    let headers = HeaderMap::new();
    let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)), 12345);
    let ip = get_client_ip(&headers, false, Some(peer));
    assert_eq!(ip, "192.168.1.50");
}

#[test]
fn ignores_spoofed_forwarded_for_without_trust_proxy() {
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "203.0.113.99".parse().expect("header"));
    let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 8080);
    let ip = get_client_ip(&headers, false, Some(peer));
    assert_eq!(ip, "10.0.0.1");
}

#[test]
fn trusts_forwarded_for_when_proxy_trusted() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-forwarded-for",
        "203.0.113.99, 10.0.0.1".parse().expect("header"),
    );
    let ip = get_client_ip(&headers, true, None);
    assert_eq!(ip, "203.0.113.99");
}

#[test]
fn falls_back_to_unknown_without_peer_or_headers() {
    let headers = HeaderMap::new();
    let ip = get_client_ip(&headers, false, None);
    assert_eq!(ip, "unknown");
}
