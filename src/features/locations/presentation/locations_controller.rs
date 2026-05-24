use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::features::campaigns::presentation::{CampaignContext, SlugIdPath};
use crate::features::locations::application::GeoService;
use crate::features::locations::infrastructure::{location_to_json, LocationRepository};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct LocationBody {
    pub name: String,
    #[serde(rename = "centerLat")]
    pub center_lat: f64,
    #[serde(rename = "centerLng")]
    pub center_lng: f64,
    #[serde(rename = "radiusMeters")]
    pub radius_meters: f64,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

pub async fn list_locations(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
) -> ApiResult<Json<SuccessResponse<Vec<Value>>>> {
    let locations = LocationRepository::find_all(&state, &ctx.paths).await?;
    Ok(SuccessResponse::data(
        locations.iter().map(location_to_json).collect(),
    ))
}

pub async fn create_location(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Json(body): Json<LocationBody>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    validate_location_body(&body)?;
    let loc = LocationRepository::create(
        &state,
        &ctx.paths,
        body.name.trim(),
        body.center_lat,
        body.center_lng,
        body.radius_meters,
        body.enabled,
    )
    .await?;
    Ok(SuccessResponse::data(location_to_json(&loc)))
}

pub async fn update_location(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Path(path): Path<SlugIdPath>,
    Json(body): Json<LocationBody>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    validate_location_body(&body)?;
    if LocationRepository::find_by_id(&state, &ctx.paths, &path.id)
        .await?
        .is_none()
    {
        return Err(ApiError::bad_request("Location not found."));
    }
    let payload = serde_json::Map::from_iter([
        ("name".into(), serde_json::json!(body.name.trim())),
        ("centerLat".into(), serde_json::json!(body.center_lat)),
        ("centerLng".into(), serde_json::json!(body.center_lng)),
        ("radiusMeters".into(), serde_json::json!(body.radius_meters)),
        ("enabled".into(), serde_json::json!(body.enabled)),
    ]);
    let loc = LocationRepository::update(&state, &ctx.paths, &path.id, payload).await?;
    Ok(SuccessResponse::data(location_to_json(&loc)))
}

pub async fn delete_location(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Path(path): Path<SlugIdPath>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    if LocationRepository::find_by_id(&state, &ctx.paths, &path.id)
        .await?
        .is_none()
    {
        return Err(ApiError::bad_request("Location not found."));
    }
    LocationRepository::delete(&state, &ctx.paths, &path.id).await?;
    Ok(SuccessResponse::message("Location deleted."))
}

fn validate_location_body(body: &LocationBody) -> ApiResult<()> {
    if body.name.trim().is_empty() {
        return Err(ApiError::bad_request("name is required."));
    }
    if body.radius_meters <= 0.0 {
        return Err(ApiError::bad_request("radiusMeters must be positive."));
    }
    GeoService::validate_coordinates(body.center_lat, body.center_lng)
        .map_err(ApiError::bad_request)?;
    Ok(())
}
