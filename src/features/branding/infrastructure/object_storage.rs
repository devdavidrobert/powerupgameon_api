use crate::app_state::AppState;
use crate::error::ApiResult;
use crate::features::media::domain::extension_for_content_type;
use crate::features::media::infrastructure::upload_public_image;

pub struct BrandLogoUploadResult {
    pub url: String,
}

pub async fn upload_campaign_brand_logo(
    state: &AppState,
    campaign_slug: &str,
    content_type: &str,
    bytes: &[u8],
) -> ApiResult<BrandLogoUploadResult> {
    let ext = extension_for_content_type(content_type)
        .ok_or_else(|| crate::error::ApiError::bad_request("Unsupported logo content type."))?;
    let object_path = format!(
        "campaigns/{campaign_slug}/brand-logos/{}-{}.{}",
        chrono::Utc::now().timestamp_millis(),
        uuid::Uuid::new_v4(),
        ext
    );

    let url = upload_public_image(state, object_path, content_type, bytes).await?;
    Ok(BrandLogoUploadResult { url })
}
