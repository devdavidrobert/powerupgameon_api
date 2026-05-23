use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::domain::Campaign;

pub fn assert_challenge_open_for_campaign(campaign: &Campaign) -> ApiResult<()> {
    let now = chrono::Utc::now().timestamp_millis();

    if let Some(start) = campaign.challenge_start_time {
        if now < start {
            return Err(ApiError::with_code(
                axum::http::StatusCode::FORBIDDEN,
                "CHALLENGE_NOT_STARTED",
                "The challenge has not started yet.",
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
