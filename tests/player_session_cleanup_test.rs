mod common;

use powerupgameon_api::features::inventory::domain::resolve_inventory_decrement;
use serde_json::{json, Map};

#[test]
fn resolve_inventory_decrement_skips_pending_and_nothing_prizes() {
    let prizes = vec![json!({
        "id": "p1",
        "name": "T-Shirt",
        "isRealPrize": true
    })
    .as_object()
    .unwrap()
    .clone()];

    let pending: Map<_, _> = json!({ "prize": "pending" }).as_object().unwrap().clone();
    assert!(resolve_inventory_decrement(&pending, &prizes).is_none());

    let nothing: Map<_, _> = json!({ "prize": "Nothing" }).as_object().unwrap().clone();
    assert!(resolve_inventory_decrement(&nothing, &prizes).is_none());
}

#[test]
fn resolve_inventory_decrement_returns_location_and_prize_for_real_win() {
    let prizes = vec![json!({
        "id": "p1",
        "name": "T-Shirt",
        "isRealPrize": true
    })
    .as_object()
    .unwrap()
    .clone()];

    let sub: Map<_, _> = json!({
        "prize": "T-Shirt",
        "locationId": "loc-a"
    })
    .as_object()
    .unwrap()
    .clone();

    assert_eq!(
        resolve_inventory_decrement(&sub, &prizes),
        Some(("loc-a".into(), "p1".into()))
    );
}

#[test]
fn campaign_player_rate_limit_keys_include_registration_submission_and_spin() {
    use powerupgameon_api::middleware::rate_limit::campaign_player_rate_limit_keys;

    let config = common::test_config();
    let keys = campaign_player_rate_limit_keys(&config, "campaign-1", "10.0.0.1");

    assert_eq!(keys.len(), 3);
    assert!(keys.iter().any(|k| k.starts_with("rl_reg:")));
    assert!(keys.iter().any(|k| k.starts_with("rl_sub:")));
    assert!(keys.iter().any(|k| k.starts_with("rl_spin:")));
    assert!(keys.iter().all(|k| k.contains("campaign-1:10.0.0.1")));
}

#[tokio::test]
async fn clear_campaign_player_rate_limits_resets_in_memory_counters() {
    use powerupgameon_api::middleware::rate_limit::{
        campaign_player_rate_limit_keys, check_rate_limit_config, clear_campaign_player_rate_limits,
        RateLimitRule,
    };
    use std::time::Duration;

    let mut config = common::test_config();
    config.rate_limit_enabled = true;
    let rule = RateLimitRule {
        prefix: "rl_reg",
        window: Duration::from_secs(60 * 60),
        max: 1,
    };
    let scope_key = "campaign-1:10.0.0.55";

    check_rate_limit_config(&config, &None, scope_key, &rule)
        .await
        .expect("first request");
    assert!(
        check_rate_limit_config(&config, &None, scope_key, &rule)
            .await
            .is_err()
    );

    clear_campaign_player_rate_limits(&config, &None, "campaign-1", "10.0.0.55").await;

    check_rate_limit_config(&config, &None, scope_key, &rule)
        .await
        .expect("counter cleared after admin delete");
    assert_eq!(
        campaign_player_rate_limit_keys(&config, "campaign-1", "10.0.0.55").len(),
        3
    );
}

#[test]
fn resolve_inventory_decrement_prefers_prize_id_on_submission() {
    let prizes = vec![];

    let sub: Map<_, _> = json!({
        "prize": "T-Shirt",
        "prizeId": "p1",
        "locationId": "loc-a"
    })
    .as_object()
    .unwrap()
    .clone();

    assert_eq!(
        resolve_inventory_decrement(&sub, &prizes),
        Some(("loc-a".into(), "p1".into()))
    );
}

#[test]
fn resolve_inventory_decrement_ignores_consolation_prizes() {
    let prizes = vec![json!({
        "id": "p2",
        "name": "Sticker",
        "isRealPrize": false
    })
    .as_object()
    .unwrap()
    .clone()];

    let sub: Map<_, _> = json!({ "prize": "Sticker" }).as_object().unwrap().clone();
    assert!(resolve_inventory_decrement(&sub, &prizes).is_none());

    let sub_with_id: Map<_, _> = json!({
        "prize": "Sticker",
        "prizeId": "p2",
        "isRealPrize": false
    })
    .as_object()
    .unwrap()
    .clone();
    assert!(resolve_inventory_decrement(&sub_with_id, &prizes).is_none());
}