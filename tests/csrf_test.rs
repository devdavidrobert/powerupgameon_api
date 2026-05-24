mod common;

use common::{sign_csrf_payload, test_config};
use powerupgameon_api::middleware::csrf::{mint_csrf_token, verify_csrf_token};

#[test]
fn mint_and_verify_csrf_token() {
    let config = test_config();
    let token = mint_csrf_token(&config).expect("mint");
    assert!(verify_csrf_token(&config, &token));
}

#[test]
fn rejects_empty_csrf_token() {
    let config = test_config();
    assert!(!verify_csrf_token(&config, ""));
}

#[test]
fn rejects_bad_csrf_signature() {
    let config = test_config();
    let token = mint_csrf_token(&config).expect("mint");
    let mut bad = token.clone();
    bad.pop();
    bad.push('x');
    assert!(!verify_csrf_token(&config, &bad));
}

#[test]
fn rejects_expired_csrf_token() {
    let config = test_config();
    let token = sign_csrf_payload(&config.api_csrf_secret, 0, "abcd1234abcd1234");
    assert!(!verify_csrf_token(&config, &token));
}
