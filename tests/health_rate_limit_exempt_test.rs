use powerupgameon_api::middleware::rate_limit::{
    check_rate_limit_config, is_global_rate_limit_exempt, RateLimitRule,
};
use std::time::Duration;

mod common;

#[test]
fn health_and_csrf_paths_are_exempt_from_global_limit() {
    assert!(is_global_rate_limit_exempt("/health"));
    assert!(is_global_rate_limit_exempt("/api/csrf-token"));
    assert!(!is_global_rate_limit_exempt("/api/other"));
}

#[tokio::test]
async fn exempt_paths_do_not_consume_global_budget() {
    let mut config = common::test_config();
    config.rate_limit_enabled = true;
    config.global_rate_limit_max = 1;
    let rule = RateLimitRule {
        prefix: "rl_global",
        window: Duration::from_secs(config.global_rate_limit_window_secs),
        max: config.global_rate_limit_max,
    };

    // Simulates health/csrf being skipped before check_rate_limit runs.
    for path in ["/health", "/api/csrf-token"] {
        assert!(is_global_rate_limit_exempt(path));
    }

    check_rate_limit_config(&config, &None, "127.0.0.1", &rule)
        .await
        .expect("first non-exempt request");
    assert!(
        check_rate_limit_config(&config, &None, "127.0.0.1", &rule)
            .await
            .is_err()
    );
}
