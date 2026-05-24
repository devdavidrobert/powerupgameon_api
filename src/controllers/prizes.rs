use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::features::campaigns::domain::CampaignStatus;
use crate::features::campaigns::presentation::{CampaignContext, PublicCampaignContext};
use crate::features::spin::domain::is_consolation_prize;
use crate::models::prize::PrizeModel;
use crate::utils::firestore::document_id_from_map;
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::sync::Arc;

pub async fn get_all_prizes(
    State(state): State<Arc<AppState>>,
    PublicCampaignContext(ctx): PublicCampaignContext,
) -> ApiResult<(HeaderMap, Json<SuccessResponse<Vec<Map<String, Value>>>>)> {
    let prizes = PrizeModel::find_all(&state, &ctx.paths).await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CACHE_CONTROL,
        "public, max-age=30, stale-while-revalidate=120"
            .parse()
            .unwrap(),
    );
    Ok((headers, SuccessResponse::data(prizes)))
}

pub async fn get_prize(
    State(state): State<Arc<AppState>>,
    PublicCampaignContext(ctx): PublicCampaignContext,
    Path(id): Path<String>,
) -> ApiResult<Json<SuccessResponse<Map<String, Value>>>> {
    let prize = PrizeModel::find_by_id(&state, &ctx.paths, &id)
        .await?
        .ok_or_else(|| ApiError::bad_request("Prize not found."))?;
    Ok(SuccessResponse::data(prize))
}

#[derive(Deserialize)]
pub struct PrizeBody {
    pub name: Option<String>,
    #[serde(rename = "isRealPrize")]
    pub is_real_prize: Option<bool>,
    pub order: Option<i64>,
}

pub async fn get_all_prizes_admin(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
) -> ApiResult<Json<SuccessResponse<Vec<Map<String, Value>>>>> {
    Ok(SuccessResponse::data(
        PrizeModel::find_all(&state, &ctx.paths).await?,
    ))
}

pub async fn create_prize(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Json(body): Json<PrizeBody>,
) -> ApiResult<(StatusCode, Json<SuccessResponse<Map<String, Value>>>)> {
    let name = body.name.as_deref().unwrap_or("").trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("name is required."));
    }
    let all = PrizeModel::find_all(&state, &ctx.paths).await?;
    let order = body.order.unwrap_or(all.len() as i64 + 1);
    let mut data = Map::new();
    data.insert("name".into(), json!(name));
    data.insert(
        "isRealPrize".into(),
        json!(body.is_real_prize.unwrap_or(true)),
    );
    data.insert("order".into(), json!(order));
    let prize = PrizeModel::create(&state, &ctx.paths, data).await?;
    Ok((StatusCode::CREATED, SuccessResponse::data(prize)))
}

pub async fn update_prize(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Path(id): Path<String>,
    Json(body): Json<PrizeBody>,
) -> ApiResult<Json<SuccessResponse<Map<String, Value>>>> {
    if PrizeModel::find_by_id(&state, &ctx.paths, &id)
        .await?
        .is_none()
    {
        return Err(ApiError::bad_request("Prize not found."));
    }
    let mut updates = Map::new();
    if let Some(name) = body.name {
        updates.insert("name".into(), json!(name.trim()));
    }
    if let Some(is_real) = body.is_real_prize {
        updates.insert("isRealPrize".into(), json!(is_real));
    }
    if let Some(order) = body.order {
        updates.insert("order".into(), json!(order));
    }
    let updated = PrizeModel::update(&state, &ctx.paths, &id, updates).await?;
    Ok(SuccessResponse::data(updated))
}

pub async fn delete_prize(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Path(id): Path<String>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    if PrizeModel::find_by_id(&state, &ctx.paths, &id)
        .await?
        .is_none()
    {
        return Err(ApiError::bad_request("Prize not found."));
    }

    if ctx.campaign.status == CampaignStatus::Active {
        let all = PrizeModel::find_all(&state, &ctx.paths).await?;
        let target = all
            .iter()
            .find(|p| document_id_from_map(p).as_deref() == Some(id.as_str()));
        if let Some(prize) = target {
            if is_consolation_prize(prize) {
                let consolation_count = all.iter().filter(|p| is_consolation_prize(p)).count();
                if consolation_count <= 1 {
                    return Err(ApiError::with_code(
                        StatusCode::BAD_REQUEST,
                        "NO_CONSOLATION_PRIZES",
                        "Cannot delete the last consolation prize on an active campaign.",
                    ));
                }
            }
        }
    }

    PrizeModel::delete(&state, &ctx.paths, &id).await?;
    Ok(SuccessResponse::message("Prize deleted."))
}
