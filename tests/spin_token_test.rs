mod common;

use common::{sign_spin_payload, test_config};
use powerupgameon_api::error::ApiError;
use powerupgameon_api::utils::spin_token::{mint_spin_token, verify_spin_token};

#[test]
fn mint_and_verify_spin_token() {
    let config = test_config();
    let token = mint_spin_token(&config, "session-123").expect("mint");
    let sid = verify_spin_token(&config, &token).expect("verify");
    assert_eq!(sid, "session-123");
}

#[test]
fn rejects_empty_spin_token() {
    let config = test_config();
    let err = verify_spin_token(&config, "").unwrap_err();
    match err {
        ApiError::WithStatus { code, .. } => {
            assert_eq!(code.as_deref(), Some("SPIN_TOKEN_INVALID"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn rejects_bad_spin_signature() {
    let config = test_config();
    let token = mint_spin_token(&config, "session-123").expect("mint");
    let mut bad = token.clone();
    bad.pop();
    bad.push('x');
    let err = verify_spin_token(&config, &bad).unwrap_err();
    match err {
        ApiError::WithStatus { code, .. } => {
            assert_eq!(code.as_deref(), Some("SPIN_TOKEN_INVALID"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn rejects_expired_spin_token() {
    let config = test_config();
    let token = sign_spin_payload(&config.spin_token_secret, "session-123", 0);
    let err = verify_spin_token(&config, &token).unwrap_err();
    match err {
        ApiError::WithStatus {
            status,
            code,
            ..
        } => {
            assert_eq!(status, axum::http::StatusCode::UNAUTHORIZED);
            assert_eq!(code.as_deref(), Some("SPIN_TOKEN_EXPIRED"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}
