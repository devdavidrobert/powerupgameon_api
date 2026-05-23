use crate::app_state::AppState;
use crate::config::Config;
use crate::models::settings::SettingsModel;
use crate::error::{ApiError, ApiResult};

pub async fn assert_challenge_open(state: &AppState) -> ApiResult<()> {
    let settings = SettingsModel::get(state).await?;
    let now = chrono::Utc::now().timestamp_millis();

    if let Some(start) = settings.challenge_start_time {
        if now < start {
            return Err(ApiError::with_code(
                axum::http::StatusCode::FORBIDDEN,
                "CHALLENGE_NOT_STARTED",
                "The challenge has not started yet.",
            ));
        }
    }

    if let Some(end) = settings.challenge_end_time {
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

pub async fn assert_challenge_open_config(config: &Config, state: &AppState) -> ApiResult<()> {
    let _ = config;
    assert_challenge_open(state).await
}
