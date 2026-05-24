use crate::app_state::AppState;
use crate::error::{ApiResult, SuccessResponse};
use crate::features::branding::infrastructure::upload_campaign_brand_logo;
use crate::features::campaigns::presentation::CampaignContext;
use crate::features::media::application::read_uploaded_image;
use axum::extract::{Multipart, State};
use axum::Json;
use serde_json::json;
use std::sync::Arc;

pub async fn upload_brand_logo(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    mut multipart: Multipart,
) -> ApiResult<Json<SuccessResponse<serde_json::Value>>> {
    let uploaded = read_uploaded_image(&mut multipart).await?;
    let result =
        upload_campaign_brand_logo(&state, ctx.slug(), &uploaded.content_type, &uploaded.bytes)
            .await?;

    Ok(SuccessResponse::data(json!({ "url": result.url })))
}
