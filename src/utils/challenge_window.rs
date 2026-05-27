use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::domain::Campaign;

pub fn assert_challenge_open_for_campaign(campaign: &Campaign) -> ApiResult<()> {
    let now = chrono::Utc::now().timestamp_millis();

    if let Some(start) = campaign.challenge_start_time {
        if now < start {
            return Err(ApiError::with_code_data(
                axum::http::StatusCode::FORBIDDEN,
                "CHALLENGE_NOT_STARTED",
                "The challenge has not started yet.",
                serde_json::json!({ "challengeStartTime": start }),
            ));
        }
    }

    if let Some(end) = campaign.challenge_end_time {
        if now > end {
            return Err(ApiError::with_code(
                axum::http::StatusCode::FORBIDDEN,
                "CHALLENGE_ENDED",
                "The challenge has ended.",
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ApiError;
    use crate::features::campaigns::domain::{
        Campaign, CampaignStatus, GeoEnforcement, StaggerMode,
    };

    fn sample_campaign(start: Option<i64>, end: Option<i64>) -> Campaign {
        Campaign {
            id: "camp-1".into(),
            slug: "test".into(),
            name: "Test".into(),
            status: CampaignStatus::Active,
            challenge_start_time: start,
            challenge_end_time: end,
            stagger_mode: StaggerMode::Linear,
            stagger_schedule: None,
            geo_enforcement: GeoEnforcement::Reject,
            spin_pass_percent: 100,
            brand_logos: None,
            player_outcome_copy: None,
            registration_form_header: None,
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn challenge_open_when_no_timers_are_set() {
        assert!(assert_challenge_open_for_campaign(&sample_campaign(None, None)).is_ok());
    }

    #[test]
    fn challenge_not_started_before_start_time() {
        let start = chrono::Utc::now().timestamp_millis() + 60_000;
        let err =
            assert_challenge_open_for_campaign(&sample_campaign(Some(start), None)).unwrap_err();
        assert!(matches!(
            err,
            ApiError::WithStatus {
                code: Some(ref code),
                data: Some(ref data),
                ..
            } if code == "CHALLENGE_NOT_STARTED"
                && data.get("challengeStartTime").and_then(|v| v.as_i64()) == Some(start)
        ));
    }

    #[test]
    fn challenge_ended_after_end_time() {
        let end = chrono::Utc::now().timestamp_millis() - 60_000;
        let err =
            assert_challenge_open_for_campaign(&sample_campaign(None, Some(end))).unwrap_err();
        assert!(matches!(
            err,
            ApiError::WithStatus {
                code: Some(ref code),
                ..
            } if code == "CHALLENGE_ENDED"
        ));
    }

    #[test]
    fn challenge_open_during_active_window() {
        let start = chrono::Utc::now().timestamp_millis() - 60_000;
        let end = chrono::Utc::now().timestamp_millis() + 60_000;
        assert!(
            assert_challenge_open_for_campaign(&sample_campaign(Some(start), Some(end))).is_ok()
        );
    }
}
