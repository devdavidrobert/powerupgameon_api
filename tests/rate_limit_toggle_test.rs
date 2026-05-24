mod common;

use common::test_config;
use powerupgameon_api::middleware::rate_limit::{check_rate_limit_config, RateLimitRule};
use std::time::Duration;

fn global_rule_for_config(config: &powerupgameon_api::config::Config) -> RateLimitRule {
    RateLimitRule {
        prefix: "rl_global",
        window: Duration::from_secs(config.global_rate_limit_window_secs),
        max: config.global_rate_limit_max,
    }
}

#[tokio::test]
async fn rate_limit_disabled_allows_unlimited_requests() {
    let mut config = test_config();
    config.rate_limit_enabled = false;
    let rule = global_rule_for_config(&config);

    for _ in 0..500 {
        check_rate_limit_config(&config, &None, "127.0.0.1", &rule)
            .await
            .expect("should not rate limit when disabled");
    }
}

#[tokio::test]
async fn rate_limit_enabled_blocks_after_max() {
    let mut config = test_config();
    config.rate_limit_enabled = true;
    config.registration_rate_limit_max = 2;
    let rule = RateLimitRule {
        prefix: "rl_reg",
        window: Duration::from_secs(60 * 60),
        max: config.registration_rate_limit_max,
    };

    check_rate_limit_config(&config, &None, "10.0.0.99", &rule)
        .await
        .expect("first");
    check_rate_limit_config(&config, &None, "10.0.0.99", &rule)
        .await
        .expect("second");
    assert!(
        check_rate_limit_config(&config, &None, "10.0.0.99", &rule)
            .await
            .is_err()
    );
}
