use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::features::campaigns::application::CampaignService;
use crate::features::campaigns::domain::{CampaignStatus, GeoEnforcement, StaggerMode};
use crate::features::campaigns::infrastructure::{
    build_update_payload, campaign_to_json, parse_stagger_schedule, validate_slug,
    CampaignRepository, CampaignUpdateInput,
};
use crate::features::campaigns::presentation::campaign_context::{CampaignContext, SlugPath};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct CreateCampaignBody {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateCampaignBody {
    pub name: Option<String>,
    pub status: Option<String>,
    #[serde(rename = "challengeStartTime")]
    pub challenge_start_time: Option<Option<String>>,
    #[serde(rename = "challengeEndTime")]
    pub challenge_end_time: Option<Option<String>>,
    #[serde(rename = "staggerMode")]
    pub stagger_mode: Option<String>,
    #[serde(rename = "staggerSchedule")]
    pub stagger_schedule: Option<Value>,
    #[serde(rename = "geoEnforcement")]
    pub geo_enforcement: Option<String>,
}

pub async fn list_campaigns(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<SuccessResponse<Vec<Value>>>> {
    let campaigns = CampaignRepository::find_all(&state).await?;
    let data: Vec<Value> = campaigns.iter().map(campaign_to_json).collect();
    Ok(SuccessResponse::data(data))
}

pub async fn create_campaign(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateCampaignBody>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    validate_slug(&body.slug)?;
    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("name is required."));
    }
    let status = CampaignStatus::from_str(body.status.as_deref().unwrap_or("draft"));
    let campaign = CampaignRepository::create(&state, &body.slug, name, status).await?;
    Ok(SuccessResponse::data(campaign_to_json(&campaign)))
}

pub async fn get_campaign(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    let ctx = SlugPath { slug }.into_context(&state).await?;
    Ok(SuccessResponse::data(campaign_to_json(&ctx.campaign)))
}

pub async fn update_campaign(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    Json(body): Json<UpdateCampaignBody>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    let ctx = SlugPath { slug: slug.clone() }.into_context(&state).await?;

    let mut input = CampaignUpdateInput::default();
    if let Some(name) = body.name {
        input.name = Some(name);
    }
    if let Some(status) = body.status {
        input.status = Some(CampaignStatus::from_str(&status));
    }
    if let Some(start) = body.challenge_start_time {
        input.challenge_start_time = Some(parse_optional_time(start)?);
    }
    if let Some(end) = body.challenge_end_time {
        input.challenge_end_time = Some(parse_optional_time(end)?);
    }
    if let Some(mode) = body.stagger_mode {
        input.stagger_mode = Some(StaggerMode::from_str(&mode));
    }
    if let Some(schedule) = body.stagger_schedule {
        input.stagger_schedule = Some(parse_stagger_schedule(&schedule)?);
    }
    if let Some(geo) = body.geo_enforcement {
        input.geo_enforcement = Some(GeoEnforcement::from_str(&geo));
    }

    let payload = build_update_payload(&input)?;
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

    let updated = CampaignRepository::update(&state, ctx.campaign_id(), payload).await?;
    CampaignService::invalidate_slug(&slug);
    Ok(SuccessResponse::data(campaign_to_json(&updated)))
}

pub async fn archive_campaign(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    let ctx = SlugPath { slug: slug.clone() }.into_context(&state).await?;
    CampaignRepository::archive(&state, ctx.campaign_id()).await?;
    CampaignService::invalidate_slug(&slug);
    Ok(SuccessResponse::message("Campaign archived."))
}

pub async fn get_campaign_settings(
    ctx: PublicCampaignContext,
) -> ApiResult<(axum::http::HeaderMap, Json<SuccessResponse<Value>>)> {
    let PublicCampaignContext(ctx) = ctx;
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        "public, max-age=30, stale-while-revalidate=60".parse().unwrap(),
    );
    Ok((
        headers,
        SuccessResponse::data(json!({
            "challengeStartTime": ctx.campaign.challenge_start_time,
            "challengeEndTime": ctx.campaign.challenge_end_time,
            "updatedAt": ctx.campaign.updated_at,
        })),
    ))
}

pub async fn update_campaign_settings(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Json(body): Json<UpdateCampaignBody>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    let mut input = CampaignUpdateInput::default();
    if let Some(start) = body.challenge_start_time {
        input.challenge_start_time = Some(parse_optional_time(start)?);
    }
    if let Some(end) = body.challenge_end_time {
        input.challenge_end_time = Some(parse_optional_time(end)?);
    }
    if let Some(mode) = body.stagger_mode {
        input.stagger_mode = Some(StaggerMode::from_str(&mode));
    }
    if let Some(schedule) = body.stagger_schedule {
        input.stagger_schedule = Some(parse_stagger_schedule(&schedule)?);
    }
    if let Some(geo) = body.geo_enforcement {
        input.geo_enforcement = Some(GeoEnforcement::from_str(&geo));
    }

    let payload = build_update_payload(&input)?;
    let updated = CampaignRepository::update(&state, ctx.campaign_id(), payload).await?;
    CampaignService::invalidate_slug(ctx.slug());
    Ok(SuccessResponse::data(json!({
        "challengeStartTime": updated.challenge_start_time,
        "challengeEndTime": updated.challenge_end_time,
        "updatedAt": updated.updated_at,
        "staggerMode": updated.stagger_mode.as_str(),
        "staggerSchedule": updated.stagger_schedule,
        "geoEnforcement": updated.geo_enforcement.as_str(),
    })))
}

pub async fn clear_campaign_timers(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    let payload = Map::from_iter([
        ("challengeStartTime".into(), Value::Null),
        ("challengeEndTime".into(), Value::Null),
    ]);
    CampaignRepository::update(&state, ctx.campaign_id(), payload).await?;
    CampaignService::invalidate_slug(ctx.slug());
    Ok(SuccessResponse::message(
        "Timers cleared. Campaign is now open indefinitely.",
    ))
}

use crate::features::campaigns::presentation::campaign_context::PublicCampaignContext;

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
