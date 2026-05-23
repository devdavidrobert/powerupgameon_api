use powerupgameon_api::features::campaigns::infrastructure::{
    parse_challenge_time_value, validate_challenge_window,
};
use serde_json::{json, Map};

#[test]
fn parse_challenge_time_accepts_unix_millis() {
    let parsed = parse_challenge_time_value(&json!(1779649200000i64)).unwrap();
    assert_eq!(parsed.as_i64(), Some(1779649200000));
}

#[test]
fn parse_challenge_time_accepts_rfc3339_string() {
    let parsed = parse_challenge_time_value(&json!("2026-05-24T19:00:00.000Z")).unwrap();
    assert_eq!(parsed.as_i64(), Some(1779649200000));
}

#[test]
fn parse_challenge_time_accepts_datetime_local_string() {
    let parsed = parse_challenge_time_value(&json!("2026-05-24T22:00")).unwrap();
    assert!(parsed.as_i64().is_some());
}

#[test]
fn validate_challenge_window_rejects_end_before_start() {
    let mut payload = Map::new();
    payload.insert("challengeStartTime".into(), json!(2000));
    payload.insert("challengeEndTime".into(), json!(1000));
    assert!(validate_challenge_window(&payload).is_err());
}

#[test]
fn validate_challenge_window_allows_valid_range() {
    let mut payload = Map::new();
    payload.insert("challengeStartTime".into(), json!(1000));
    payload.insert("challengeEndTime".into(), json!(2000));
    assert!(validate_challenge_window(&payload).is_ok());
}

#[test]
fn parse_challenge_time_accepts_float_whole_number() {
    let parsed = parse_challenge_time_value(&json!(1779649200000f64)).unwrap();
    assert_eq!(parsed.as_i64(), Some(1779649200000));
}
