use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::features::campaigns::domain::CampaignStatus;
use crate::features::campaigns::presentation::{
    CampaignContext, PublicCampaignContext, SlugIdPath,
};
use crate::features::media::application::read_uploaded_image;
use crate::features::media::domain::extension_for_content_type;
use crate::features::media::infrastructure::upload_public_image;
use crate::features::spin::domain::is_consolation_prize;
use crate::models::prize::PrizeModel;
use crate::utils::firestore::document_id_from_map;
use axum::{
    extract::{Multipart, Path, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::sync::Arc;

fn apply_image_url_update(updates: &mut Map<String, Value>, image_url: &Value) -> ApiResult<()> {
    if image_url.is_null() {
        updates.insert("imageUrl".into(), Value::Null);
        return Ok(());
    }
    let Some(url) = image_url.as_str() else {
        return Err(ApiError::bad_request("imageUrl must be a string or null."));
    };
    let trimmed = url.trim();
    if trimmed.is_empty() {
        updates.insert("imageUrl".into(), Value::Null);
        return Ok(());
    }
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err(ApiError::bad_request(
            "imageUrl must start with http:// or https://.",
        ));
    }
    updates.insert("imageUrl".into(), json!(trimmed));
    Ok(())
}

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
    Path(path): Path<SlugIdPath>,
) -> ApiResult<Json<SuccessResponse<Map<String, Value>>>> {
    let prize = PrizeModel::find_by_id(&state, &ctx.paths, &path.id)
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
    #[serde(rename = "imageUrl")]
    pub image_url: Option<Value>,
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
    if let Some(image_url) = body.image_url.as_ref() {
        apply_image_url_update(&mut data, image_url)?;
    }
    let prize = PrizeModel::create(&state, &ctx.paths, data).await?;
    Ok((StatusCode::CREATED, SuccessResponse::data(prize)))
}

pub async fn update_prize(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Path(path): Path<SlugIdPath>,
    Json(body): Json<PrizeBody>,
) -> ApiResult<Json<SuccessResponse<Map<String, Value>>>> {
    if PrizeModel::find_by_id(&state, &ctx.paths, &path.id)
        .await?
        .is_none()
    {
        return Err(ApiError::bad_request("Prize not found."));
    }
    let mut updates = Map::new();
    if let Some(name) = body.name {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(ApiError::bad_request("name cannot be empty."));
        }
        updates.insert("name".into(), json!(trimmed));
    }
    if let Some(is_real) = body.is_real_prize {
        updates.insert("isRealPrize".into(), json!(is_real));
    }
    if let Some(order) = body.order {
        updates.insert("order".into(), json!(order));
    }
    if let Some(image_url) = body.image_url.as_ref() {
        apply_image_url_update(&mut updates, image_url)?;
    }

    if updates.is_empty() {
        let prize = PrizeModel::find_by_id(&state, &ctx.paths, &path.id)
            .await?
            .ok_or_else(|| ApiError::bad_request("Prize not found."))?;
        return Ok(SuccessResponse::data(prize));
    }

    let updated = PrizeModel::update(&state, &ctx.paths, &path.id, updates).await?;
    Ok(SuccessResponse::data(updated))
}

pub async fn delete_prize(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Path(path): Path<SlugIdPath>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    if PrizeModel::find_by_id(&state, &ctx.paths, &path.id)
        .await?
        .is_none()
    {
        return Err(ApiError::bad_request("Prize not found."));
    }

    if ctx.campaign.status == CampaignStatus::Active {
        let all = PrizeModel::find_all(&state, &ctx.paths).await?;
        let target = all
            .iter()
            .find(|p| document_id_from_map(p).as_deref() == Some(path.id.as_str()));
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

    PrizeModel::delete(&state, &ctx.paths, &path.id).await?;
    Ok(SuccessResponse::message("Prize deleted."))
}

pub async fn upload_prize_image(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Path(path): Path<SlugIdPath>,
    mut multipart: Multipart,
) -> ApiResult<Json<SuccessResponse<Map<String, Value>>>> {
    if PrizeModel::find_by_id(&state, &ctx.paths, &path.id)
        .await?
        .is_none()
    {
        return Err(ApiError::bad_request("Prize not found."));
    }

    let uploaded = read_uploaded_image(&mut multipart).await?;
    let ext = extension_for_content_type(&uploaded.content_type)
        .ok_or_else(|| ApiError::bad_request("Unsupported image content type."))?;
    let object_path = format!(
        "campaigns/{}/prizes/{}-{}.{}",
        ctx.slug(),
        path.id,
        uuid::Uuid::new_v4(),
        ext
    );

    let url =
        upload_public_image(&state, object_path, &uploaded.content_type, &uploaded.bytes).await?;

    let mut updates = Map::new();
    updates.insert("imageUrl".into(), json!(url));
    let prize = PrizeModel::update(&state, &ctx.paths, &path.id, updates).await?;

    Ok(SuccessResponse::data(prize))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_image_url_update_accepts_https_url() {
        let mut updates = Map::new();
        apply_image_url_update(&mut updates, &json!("https://cdn.example/prize.png")).unwrap();
        assert_eq!(
            updates.get("imageUrl").and_then(|v| v.as_str()),
            Some("https://cdn.example/prize.png")
        );
    }

    #[test]
    fn apply_image_url_update_clears_on_null() {
        let mut updates = Map::new();
        apply_image_url_update(&mut updates, &Value::Null).unwrap();
        assert_eq!(updates.get("imageUrl"), Some(&Value::Null));
    }
}
