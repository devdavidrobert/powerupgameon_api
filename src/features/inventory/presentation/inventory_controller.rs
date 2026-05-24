use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::features::campaigns::presentation::CampaignContext;
use crate::features::inventory::application::InventoryService;
use crate::features::inventory::domain::InventoryView;
use crate::features::inventory::infrastructure::{inventory_view_to_json, InventoryRepository};
use crate::features::locations::infrastructure::LocationRepository;
use crate::features::spin::domain::is_consolation_prize;
use crate::models::prize::PrizeModel;
use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct UpsertInventoryBody {
    #[serde(rename = "locationId")]
    pub location_id: String,
    #[serde(rename = "prizeId")]
    pub prize_id: String,
    #[serde(rename = "totalQuantity")]
    pub total_quantity: i64,
}

pub async fn list_inventory(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
) -> ApiResult<Json<SuccessResponse<Vec<Value>>>> {
    let views = InventoryRepository::build_views(&state, &ctx.paths, &ctx.campaign).await?;
    Ok(SuccessResponse::data(
        views.iter().map(inventory_view_to_json).collect(),
    ))
}

pub async fn upsert_inventory(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Json(body): Json<UpsertInventoryBody>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    if body.location_id.trim().is_empty() {
        return Err(ApiError::bad_request("locationId is required."));
    }
    if body.prize_id.trim().is_empty() {
        return Err(ApiError::bad_request("prizeId is required."));
    }

    let location = LocationRepository::find_by_id(&state, &ctx.paths, body.location_id.trim())
        .await?
        .ok_or_else(|| ApiError::bad_request("Location not found."))?;
    let prize = PrizeModel::find_by_id(&state, &ctx.paths, body.prize_id.trim())
        .await?
        .ok_or_else(|| ApiError::bad_request("Prize not found."))?;

    let prize_name = prize
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(body.prize_id.trim())
        .to_string();
    let reconcile = !is_consolation_prize(&prize);

    let slot = InventoryRepository::upsert_slot(
        &state,
        &ctx.paths,
        body.location_id.trim(),
        body.prize_id.trim(),
        &prize_name,
        body.total_quantity,
        reconcile,
    )
    .await?;

    let now = crate::utils::firestore::millis_now();
    let releasable = InventoryService::releasable_now(&ctx.campaign, &slot, now);
    let view = InventoryView {
        location_id: slot.location_id.clone(),
        location_name: location.name,
        prize_id: slot.prize_id.clone(),
        prize_name,
        total_quantity: slot.total_quantity,
        awarded_count: slot.awarded_count,
        releasable_now: releasable,
        remaining: slot.remaining(releasable),
    };

    Ok(SuccessResponse::data(inventory_view_to_json(&view)))
}
