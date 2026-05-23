use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::features::campaigns::presentation::CampaignContext;
use crate::features::inventory::infrastructure::{inventory_view_to_json, InventoryRepository};
use crate::features::locations::infrastructure::LocationRepository;
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
    if LocationRepository::find_by_id(&state, &ctx.paths, &body.location_id)
        .await?
        .is_none()
    {
        return Err(ApiError::bad_request("Location not found."));
    }
    if PrizeModel::find_by_id(&state, &ctx.paths, &body.prize_id)
        .await?
        .is_none()
    {
        return Err(ApiError::bad_request("Prize not found."));
    }

    let slot = InventoryRepository::upsert_slot(
        &state,
        &ctx.paths,
        &body.location_id,
        &body.prize_id,
        body.total_quantity,
    )
    .await?;

    let views = InventoryRepository::build_views(&state, &ctx.paths, &ctx.campaign).await?;
    let view = views
        .into_iter()
        .find(|v| v.location_id == slot.location_id && v.prize_id == slot.prize_id)
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("Inventory view missing")))?;

    Ok(SuccessResponse::data(inventory_view_to_json(&view)))
}
