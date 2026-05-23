#![allow(dead_code)]

use base64::Engine;
use powerupgameon_api::config::Config;
use ring::hmac;

pub fn test_config() -> Config {
    Config {
        port: 4000,
        node_env: "test".into(),
        is_production: false,
        firebase_project_id: None,
        firebase_service_account_json: None,
        allowed_origins: vec!["http://localhost:3000".into()],
        trust_proxy: false,
        rate_limit_window_ms: 15 * 60 * 1000,
        rate_limit_max: 200,
        real_prize_limit: 5,
        session_ttl_hours: 12,
        api_csrf_secret: "test-csrf-secret".into(),
        spin_token_secret: "test-spin-secret".into(),
        redis_url: None,
        allowed_admin_emails: vec!["admin@example.com".into()],
    }
}

pub fn sign_csrf_payload(secret: &str, exp: i64, nonce: &str) -> String {
    let payload_json = serde_json::json!({ "exp": exp, "nonce": nonce }).to_string();
    let payload_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload_json);
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
    let tag = hmac::sign(&key, payload_b64.as_bytes());
    let sig = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(tag.as_ref());
    format!("{payload_b64}.{sig}")
}

pub fn sign_spin_payload(secret: &str, session_id: &str, exp: i64) -> String {
    let payload_json = serde_json::json!({ "sid": session_id, "exp": exp }).to_string();
    let payload_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload_json);
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
    let tag = hmac::sign(&key, payload_b64.as_bytes());
    let sig = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(tag.as_ref());
    format!("{payload_b64}.{sig}")
}
