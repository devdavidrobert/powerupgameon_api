use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::features::campaigns::application::CampaignService;
use crate::features::campaigns::domain::{CampaignStatus, GeoEnforcement, StaggerMode};
use crate::features::campaigns::infrastructure::{
    build_update_payload, campaign_to_json, parse_brand_logos, parse_challenge_time_value,
    parse_ip_rate_limit_window_secs, parse_player_outcome_copy, parse_registration_form_header,
    parse_stagger_schedule,
    validate_challenge_window, validate_slug, CampaignRepository, CampaignUpdateInput,
};
use crate::features::campaigns::presentation::campaign_context::{CampaignContext, SlugPath};
use crate::features::locations::infrastructure::LocationRepository;
use crate::models::question::QuestionModel;
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
    pub challenge_start_time: Option<Value>,
    #[serde(rename = "challengeEndTime")]
    pub challenge_end_time: Option<Value>,
    #[serde(rename = "staggerMode")]
    pub stagger_mode: Option<String>,
    #[serde(rename = "staggerSchedule")]
    pub stagger_schedule: Option<Value>,
    #[serde(rename = "geoEnforcement")]
    pub geo_enforcement: Option<String>,
    #[serde(rename = "spinPassPercent")]
    pub spin_pass_percent: Option<i64>,
    #[serde(rename = "brandLogos")]
    pub brand_logos: Option<Value>,
    #[serde(rename = "playerOutcomeCopy")]
    pub player_outcome_copy: Option<Value>,
    #[serde(rename = "registrationFormHeader")]
    pub registration_form_header: Option<String>,
    #[serde(rename = "ipRateLimitWindowSecs")]
    pub ip_rate_limit_window_secs: Option<i64>,
}

fn apply_update_body(body: UpdateCampaignBody, input: &mut CampaignUpdateInput) -> ApiResult<()> {
    if let Some(name) = body.name {
        input.name = Some(name);
    }
    if let Some(status) = body.status {
        input.status = Some(CampaignStatus::from_str(&status));
    }
    if let Some(start) = body.challenge_start_time.as_ref() {
        input.challenge_start_time = Some(parse_challenge_time_value(start)?);
    }
    if let Some(end) = body.challenge_end_time.as_ref() {
        input.challenge_end_time = Some(parse_challenge_time_value(end)?);
    }
    if let Some(mode) = body.stagger_mode {
        let parsed = StaggerMode::from_str(&mode);
        input.stagger_mode = Some(parsed);
        if parsed != StaggerMode::Stepped {
            input.clear_stagger_schedule = true;
            input.stagger_schedule = None;
        }
    }
    if let Some(schedule) = body.stagger_schedule {
        input.stagger_schedule = Some(parse_stagger_schedule(&schedule)?);
        input.clear_stagger_schedule = false;
    }
    if let Some(geo) = body.geo_enforcement {
        input.geo_enforcement = Some(GeoEnforcement::from_str(&geo));
    }
    if let Some(spin_pass_percent) = body.spin_pass_percent {
        if !(0..=100).contains(&spin_pass_percent) {
            return Err(ApiError::bad_request(
                "spinPassPercent must be between 0 and 100.",
            ));
        }
        input.spin_pass_percent = Some(spin_pass_percent);
    }
    if let Some(logos) = body.brand_logos {
        if logos.is_null() {
            input.clear_brand_logos = true;
            input.brand_logos = None;
        } else if logos.is_array() && logos.as_array().is_some_and(|arr| arr.is_empty()) {
            input.clear_brand_logos = true;
            input.brand_logos = None;
        } else {
            input.brand_logos = Some(parse_brand_logos(&logos)?);
            input.clear_brand_logos = false;
        }
    }
    if let Some(copy) = body.player_outcome_copy {
        if copy.is_null() {
            input.clear_player_outcome_copy = true;
            input.player_outcome_copy = None;
        } else {
            input.player_outcome_copy = Some(parse_player_outcome_copy(&copy)?);
            input.clear_player_outcome_copy = false;
        }
    }
    if let Some(header) = body.registration_form_header {
        let trimmed = header.trim();
        if trimmed.is_empty() {
            input.clear_registration_form_header = true;
            input.registration_form_header = None;
        } else {
            input.registration_form_header = Some(parse_registration_form_header(trimmed)?);
            input.clear_registration_form_header = false;
        }
    }
    if let Some(window_secs) = body.ip_rate_limit_window_secs {
        input.ip_rate_limit_window_secs = Some(parse_ip_rate_limit_window_secs(window_secs)?);
    }
    Ok(())
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
    apply_update_body(body, &mut input)?;

    let payload = build_update_payload(&input)?;
    validate_challenge_window(&payload)?;

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
    QuestionModel::invalidate_list_cache(&ctx.campaign_id());
    Ok(SuccessResponse::message("Campaign archived."))
}

pub async fn get_campaign_settings(
    State(state): State<Arc<AppState>>,
    ctx: PublicCampaignContext,
) -> ApiResult<(axum::http::HeaderMap, Json<SuccessResponse<Value>>)> {
    let PublicCampaignContext(ctx) = ctx;
    let has_geofence_locations =
        LocationRepository::has_enabled_locations(&state, &ctx.paths).await?;
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        "public, max-age=30, stale-while-revalidate=60"
            .parse()
            .unwrap(),
    );
    Ok((
        headers,
        SuccessResponse::data(json!({
            "challengeStartTime": ctx.campaign.challenge_start_time,
            "challengeEndTime": ctx.campaign.challenge_end_time,
            "spinPassPercent": ctx.campaign.spin_pass_percent(),
            "brandLogos": ctx.campaign.sorted_brand_logos(),
            "playerOutcomeCopy": ctx.campaign.player_outcome_copy_or_default(),
            "registrationFormHeader": ctx.campaign.registration_form_header_or_default(),
            "hasGeofenceLocations": has_geofence_locations,
            "updatedAt": ctx.campaign.updated_at,
        })),
    ))
}

pub async fn get_campaign_settings_admin(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    let has_geofence_locations =
        LocationRepository::has_enabled_locations(&state, &ctx.paths).await?;
    Ok(SuccessResponse::data(json!({
        "challengeStartTime": ctx.campaign.challenge_start_time,
        "challengeEndTime": ctx.campaign.challenge_end_time,
        "updatedAt": ctx.campaign.updated_at,
        "staggerMode": ctx.campaign.stagger_mode.as_str(),
        "staggerSchedule": ctx.campaign.stagger_schedule,
        "geoEnforcement": ctx.campaign.geo_enforcement.as_str(),
        "spinPassPercent": ctx.campaign.spin_pass_percent(),
        "brandLogos": ctx.campaign.sorted_brand_logos(),
        "playerOutcomeCopy": ctx.campaign.player_outcome_copy_or_default(),
        "registrationFormHeader": ctx.campaign.registration_form_header_or_default(),
        "ipRateLimitWindowSecs": ctx.campaign.ip_rate_limit_window_secs(),
        "hasGeofenceLocations": has_geofence_locations,
    })))
}

pub async fn update_campaign_settings(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Json(body): Json<UpdateCampaignBody>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    let mut input = CampaignUpdateInput::default();
    apply_update_body(body, &mut input)?;

    let payload = build_update_payload(&input)?;
    validate_challenge_window(&payload)?;
    let updated = CampaignRepository::update(&state, ctx.campaign_id(), payload).await?;
    CampaignService::invalidate_slug(ctx.slug());
    Ok(SuccessResponse::data(json!({
        "challengeStartTime": updated.challenge_start_time,
        "challengeEndTime": updated.challenge_end_time,
        "updatedAt": updated.updated_at,
        "staggerMode": updated.stagger_mode.as_str(),
        "staggerSchedule": updated.stagger_schedule,
        "geoEnforcement": updated.geo_enforcement.as_str(),
        "spinPassPercent": updated.spin_pass_percent(),
        "brandLogos": updated.sorted_brand_logos(),
        "playerOutcomeCopy": updated.player_outcome_copy_or_default(),
        "registrationFormHeader": updated.registration_form_header_or_default(),
        "ipRateLimitWindowSecs": updated.ip_rate_limit_window_secs(),
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
