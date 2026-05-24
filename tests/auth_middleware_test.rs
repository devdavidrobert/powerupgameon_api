use powerupgameon_api::middleware::auth::{parse_bearer_token, AuthUser};
use powerupgameon_api::services::firebase_auth::VerifiedUser;

#[test]
fn auth_user_sets_admin_from_custom_claim() {
    let user = AuthUser::from(VerifiedUser {
        uid: "u1".into(),
        email: Some("a@b.com".into()),
        email_verified: true,
        admin: true,
    });
    assert_eq!(user.uid, "u1");
    assert_eq!(user.email.as_deref(), Some("a@b.com"));
    assert!(user.admin);
}

#[test]
fn auth_user_defaults_admin_false_when_claim_absent() {
    let user = AuthUser::from(VerifiedUser {
        uid: "u1".into(),
        email: Some("a@b.com".into()),
        email_verified: true,
        admin: false,
    });
    assert!(!user.admin);
}

#[test]
fn parse_bearer_returns_none_when_header_missing() {
    assert!(parse_bearer_token(None).is_none());
}

#[test]
fn parse_bearer_returns_none_when_not_bearer_scheme() {
    assert!(parse_bearer_token(Some("Basic abc")).is_none());
}

#[test]
fn parse_bearer_extracts_token() {
    assert_eq!(
        parse_bearer_token(Some("Bearer fake.jwt.token")),
        Some("fake.jwt.token")
    );
}
