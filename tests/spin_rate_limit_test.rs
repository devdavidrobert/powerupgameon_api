mod common;

use common::test_config;
use powerupgameon_api::middleware::rate_limit::spin_rate_limit_key;
use powerupgameon_api::utils::spin_token::mint_spin_token;

#[test]
fn spin_rate_limit_key_uses_session_from_spin_token() {
    let config = test_config();
    let token = mint_spin_token(&config, "campaign-1", "session-abc").expect("mint");
    let body = serde_json::json!({ "spinToken": token }).to_string();

    let key = spin_rate_limit_key(&config, "127.0.0.1", body.as_bytes());

    assert_eq!(key, "127.0.0.1:session-abc");
}

#[test]
fn spin_rate_limit_key_falls_back_when_token_missing() {
    let config = test_config();
    let body = serde_json::json!({}).to_string();

    let key = spin_rate_limit_key(&config, "127.0.0.1", body.as_bytes());

    assert_eq!(key, "127.0.0.1:na");
}

#[test]
fn spin_rate_limit_key_differs_per_session_from_same_ip() {
    let config = test_config();
    let token_a = mint_spin_token(&config, "campaign-1", "session-a").expect("mint");
    let token_b = mint_spin_token(&config, "campaign-1", "session-b").expect("mint");
    let body_a = serde_json::json!({ "spinToken": token_a }).to_string();
    let body_b = serde_json::json!({ "spinToken": token_b }).to_string();

    let key_a = spin_rate_limit_key(&config, "10.0.0.1", body_a.as_bytes());
    let key_b = spin_rate_limit_key(&config, "10.0.0.1", body_b.as_bytes());

    assert_ne!(key_a, key_b);
}
