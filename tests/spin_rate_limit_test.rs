mod common;

use powerupgameon_api::middleware::rate_limit::campaign_ip_rate_limit_key;

#[test]
fn campaign_ip_rate_limit_key_scopes_by_campaign_and_ip() {
    let key_a = campaign_ip_rate_limit_key("campaign-1", "127.0.0.1");
    let key_b = campaign_ip_rate_limit_key("campaign-2", "127.0.0.1");

    assert_eq!(key_a, "campaign-1:127.0.0.1");
    assert_ne!(key_a, key_b);
}

#[test]
fn campaign_ip_rate_limit_key_is_ip_only_not_session() {
    let key = campaign_ip_rate_limit_key("campaign-1", "10.0.0.1");
    assert_eq!(key, "campaign-1:10.0.0.1");
    assert!(!key.contains("session"));
}
