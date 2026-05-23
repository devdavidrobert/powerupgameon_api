use crate::config::Config;
use crate::error::{ApiError, ApiResult};
use base64::Engine;
use ring::hmac;
use serde::{Deserialize, Serialize};

const TTL_MS: i64 = 20 * 60 * 1000;

#[derive(Serialize, Deserialize)]
struct SpinPayload {
    sid: String,
    exp: i64,
}

pub fn mint_spin_token(config: &Config, session_id: &str) -> ApiResult<String> {
    assert_secret(config)?;
    let exp = chrono::Utc::now().timestamp_millis() + TTL_MS;
    let payload = SpinPayload {
        sid: session_id.to_string(),
        exp,
    };
    sign_payload(config, &payload)
}

pub fn verify_spin_token(config: &Config, token: &str) -> ApiResult<String> {
    assert_secret(config)?;
    if token.is_empty() {
        return Err(ApiError::with_code(
            axum::http::StatusCode::BAD_REQUEST,
            "SPIN_TOKEN_INVALID",
            "Invalid spin token.",
        ));
    }

    let Some((payload_b64, sig_b64)) = token.split_once('.') else {
        return Err(ApiError::with_code(
            axum::http::StatusCode::BAD_REQUEST,
            "SPIN_TOKEN_INVALID",
            "Invalid spin token.",
        ));
    };

    let expected = hmac_sign(config, payload_b64);
    if !constant_time_eq(sig_b64.as_bytes(), expected.as_bytes()) {
        return Err(ApiError::with_code(
            axum::http::StatusCode::BAD_REQUEST,
            "SPIN_TOKEN_INVALID",
            "Invalid spin token.",
        ));
    }

    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|_| {
            ApiError::with_code(
                axum::http::StatusCode::BAD_REQUEST,
                "SPIN_TOKEN_INVALID",
                "Invalid spin token.",
            )
        })?;

    let parsed: SpinPayload = serde_json::from_slice(&payload_bytes).map_err(|_| {
        ApiError::with_code(
            axum::http::StatusCode::BAD_REQUEST,
            "SPIN_TOKEN_INVALID",
            "Invalid spin token.",
        )
    })?;

    if chrono::Utc::now().timestamp_millis() > parsed.exp {
        return Err(ApiError::with_code(
            axum::http::StatusCode::UNAUTHORIZED,
            "SPIN_TOKEN_EXPIRED",
            "Spin token expired.",
        ));
    }

    Ok(parsed.sid)
}

fn sign_payload(config: &Config, payload: &SpinPayload) -> ApiResult<String> {
    let payload_json = serde_json::to_vec(payload).map_err(|e| ApiError::Internal(e.into()))?;
    let payload_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload_json);
    let sig = hmac_sign(config, &payload_b64);
    Ok(format!("{payload_b64}.{sig}"))
}

fn hmac_sign(config: &Config, payload_b64: &str) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, config.spin_token_secret.as_bytes());
    let tag = hmac::sign(&key, payload_b64.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(tag.as_ref())
}

fn assert_secret(config: &Config) -> ApiResult<()> {
    if config.is_production && config.spin_token_secret.is_empty() {
        return Err(ApiError::Internal(anyhow::anyhow!(
            "SPIN_TOKEN_SECRET must be set in production."
        )));
    }
    Ok(())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
