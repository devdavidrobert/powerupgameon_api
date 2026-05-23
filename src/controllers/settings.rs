use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::models::settings::SettingsModel;
use axum::{
    extract::State,
    http::{header, HeaderMap},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::sync::Arc;

pub async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> ApiResult<(HeaderMap, Json<SuccessResponse<Value>>)> {
    let settings = SettingsModel::get(&state).await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CACHE_CONTROL,
        "public, max-age=30, stale-while-revalidate=60".parse().unwrap(),
    );
    Ok((
        headers,
        SuccessResponse::data(json!({
            "challengeStartTime": settings.challenge_start_time,
            "challengeEndTime": settings.challenge_end_time,
            "updatedAt": settings.updated_at,
        })),
    ))
}

#[derive(Deserialize)]
pub struct SettingsBody {
    #[serde(rename = "challengeStartTime")]
    pub challenge_start_time: Option<Option<String>>,
    #[serde(rename = "challengeEndTime")]
    pub challenge_end_time: Option<Option<String>>,
}

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SettingsBody>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    let mut payload = Map::new();

    if let Some(start) = body.challenge_start_time {
        payload.insert(
            "challengeStartTime".into(),
            parse_optional_time(start)?,
        );
    }
    if let Some(end) = body.challenge_end_time {
        payload.insert("challengeEndTime".into(), parse_optional_time(end)?);
    }

    if let (Some(start), Some(end)) = (
        payload.get("challengeStartTime").and_then(|v| v.as_i64()),
        payload.get("challengeEndTime").and_then(|v| v.as_i64()),
    ) {
        if start >= end {
            return Err(ApiError::bad_request(
                "challengeEndTime must be after challengeStartTime.",
            ));
        }
    }

    let updated = SettingsModel::upsert(&state, payload).await?;
    Ok(SuccessResponse::data(json!({
        "challengeStartTime": updated.challenge_start_time,
        "challengeEndTime": updated.challenge_end_time,
        "updatedAt": updated.updated_at,
    })))
}

pub async fn clear_timers(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    SettingsModel::clear_timers(&state).await?;
    Ok(SuccessResponse::message(
        "Timers cleared. Game is now open indefinitely.",
    ))
}

fn parse_optional_time(value: Option<String>) -> ApiResult<Value> {
    match value {
        None => Ok(Value::Null),
        Some(s) if s.trim().is_empty() => Ok(Value::Null),
        Some(s) => {
            let dt = chrono::DateTime::parse_from_rfc3339(s.trim())
                .map_err(|_| ApiError::bad_request("Invalid challenge time."))?;
            Ok(json!(dt.timestamp_millis()))
        }
    }
}
