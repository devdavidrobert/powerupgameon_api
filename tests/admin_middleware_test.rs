mod common;

use common::test_config;
use powerupgameon_api::middleware::auth::{user_has_admin_access, AuthUser};

fn user(uid: &str, email: &str, admin: bool) -> AuthUser {
    AuthUser {
        uid: uid.into(),
        email: Some(email.into()),
        admin,
    }
}

#[test]
fn allows_custom_claim_admin() {
    let config = test_config();
    assert!(user_has_admin_access(
        &user("1", "user@test.com", true),
        &config.allowed_admin_emails
    ));
}

#[test]
fn allows_email_in_allowlist() {
    let config = test_config();
    assert!(user_has_admin_access(
        &user("1", "admin@example.com", false),
        &config.allowed_admin_emails
    ));
}

#[test]
fn denies_non_admin_not_on_allowlist() {
    let config = test_config();
    assert!(!user_has_admin_access(
        &user("1", "other@test.com", false),
        &config.allowed_admin_emails
    ));
}

#[test]
fn denies_when_email_missing_and_not_admin() {
    let config = test_config();
    assert!(!user_has_admin_access(
        &AuthUser {
            uid: "1".into(),
            email: None,
            admin: false,
        },
        &config.allowed_admin_emails
    ));
}
