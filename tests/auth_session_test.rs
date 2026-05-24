use powerupgameon_api::controllers::auth::build_set_cookie_header_value;
use powerupgameon_api::error::ApiError;

#[test]
fn build_set_cookie_header_accepts_valid_session_value() {
    let header = build_set_cookie_header_value("valid-session-token", 3600, false)
        .expect("valid cookie header");
    assert_eq!(
        header.to_str().expect("header str"),
        "__session=valid-session-token; HttpOnly; Path=/; SameSite=Strict; Max-Age=3600"
    );
}

#[test]
fn build_set_cookie_header_rejects_invalid_characters() {
    let err = build_set_cookie_header_value("bad\r\nvalue", 3600, false).unwrap_err();
    match err {
        ApiError::WithStatus { code, .. } => {
            assert_eq!(code.as_deref(), Some("SESSION_COOKIE_INVALID"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}
