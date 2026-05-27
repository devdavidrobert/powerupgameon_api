use powerupgameon_api::controllers::submissions::UpdateSubmissionPrizeGivenBody;
use serde_json::json;

#[test]
fn update_submission_prize_given_body_deserializes_camel_case() {
    let body: UpdateSubmissionPrizeGivenBody =
        serde_json::from_value(json!({ "prizeGiven": true })).unwrap();
    assert_eq!(body.prize_given, Some(true));
}

#[test]
fn update_submission_prize_given_body_rejects_missing_field() {
    let body: UpdateSubmissionPrizeGivenBody =
        serde_json::from_value(json!({})).unwrap();
    assert_eq!(body.prize_given, None);
}
