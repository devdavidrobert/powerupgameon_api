use crate::app_state::AppState;
use crate::config::Config;
use crate::error::json_error_code;
use axum::{
    body::Body,
    extract::State,
    http::{Method, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use base64::Engine;
use rand::RngCore;
use ring::hmac;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const TOKEN_TTL_MS: i64 = 60 * 60 * 1000;

#[derive(Serialize, Deserialize)]
struct CsrfPayload {
    exp: i64,
    nonce: String,
}

pub fn mint_csrf_token(config: &Config) -> Result<String, String> {
    assert_csrf_configured(config)?;
    let exp = chrono::Utc::now().timestamp_millis() + TOKEN_TTL_MS;
    let mut nonce_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = hex::encode(nonce_bytes);
    let payload = CsrfPayload { exp, nonce };
    sign_csrf(config, &payload).map_err(|e| e.to_string())
}

pub fn verify_csrf_token(config: &Config, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    if assert_csrf_configured(config).is_err() {
        return false;
    }

    let Some((payload_b64, sig_b64)) = token.split_once('.') else {
        return false;
    };

    let expected = hmac_sign(config, payload_b64);
    if !constant_time_eq(sig_b64.as_bytes(), expected.as_bytes()) {
        return false;
    }

    let Ok(payload_bytes) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload_b64)
    else {
        return false;
    };
    let Ok(parsed) = serde_json::from_slice::<CsrfPayload>(&payload_bytes) else {
        return false;
    };

    if chrono::Utc::now().timestamp_millis() > parsed.exp {
        return false;
    }
    parsed.nonce.len() >= 16
}

fn sign_csrf(config: &Config, payload: &CsrfPayload) -> anyhow::Result<String> {
    let payload_json = serde_json::to_vec(payload)?;
    let payload_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload_json);
    let sig = hmac_sign(config, &payload_b64);
    Ok(format!("{payload_b64}.{sig}"))
}

fn hmac_sign(config: &Config, payload_b64: &str) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, config.api_csrf_secret.as_bytes());
    let tag = hmac::sign(&key, payload_b64.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(tag.as_ref())
}

fn assert_csrf_configured(config: &Config) -> Result<(), String> {
    if config.is_production && config.api_csrf_secret.is_empty() {
        return Err("API_CSRF_SECRET must be set in production.".into());
    }
    Ok(())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

pub async fn require_csrf_middleware(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if matches!(
        req.method(),
        &Method::GET | &Method::HEAD | &Method::OPTIONS
    ) {
        return next.run(req).await;
    }

    let token = req
        .headers()
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !verify_csrf_token(&state.config, token) {
        crate::logger::log(
            &state.config,
            "warn",
            "csrf_rejected",
            serde_json::json!({ "path": req.uri().path() }),
        );
        return json_error_code(
            StatusCode::FORBIDDEN,
            "CSRF_INVALID",
            "Invalid or missing CSRF token. Call GET /api/csrf-token first.",
        );
    }

    next.run(req).await
}
