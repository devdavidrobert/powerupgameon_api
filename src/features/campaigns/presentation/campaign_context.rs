use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::application::CampaignService;
use crate::features::campaigns::domain::Campaign;
use crate::features::campaigns::infrastructure::CampaignPaths;
use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};
use std::future::Future;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct CampaignContext {
    pub campaign: Campaign,
    pub paths: CampaignPaths,
}

impl CampaignContext {
    pub fn campaign_id(&self) -> &str {
        &self.campaign.id
    }

    pub fn slug(&self) -> &str {
        &self.campaign.slug
    }
}

#[derive(Clone, Debug)]
pub struct PublicCampaignContext(pub CampaignContext);

impl<S> FromRequestParts<S> for PublicCampaignContext
where
    S: Send + Sync,
    Arc<AppState>: FromRef<S>,
{
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let path = parts.uri.path().to_string();
        let state = Arc::<AppState>::from_ref(state);
        async move {
            let ctx = load_campaign_context(&path, &state).await?;
            if !ctx.campaign.is_publicly_accessible() {
                return Err(ApiError::with_code(
                    axum::http::StatusCode::FORBIDDEN,
                    "CAMPAIGN_NOT_ACTIVE",
                    "This campaign is not currently active.",
                ));
            }
            Ok(PublicCampaignContext(ctx))
        }
    }
}

impl<S> FromRequestParts<S> for CampaignContext
where
    S: Send + Sync,
    Arc<AppState>: FromRef<S>,
{
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let path = parts.uri.path().to_string();
        let state = Arc::<AppState>::from_ref(state);
        async move { load_campaign_context(&path, &state).await }
    }
}

async fn load_campaign_context(path: &str, state: &Arc<AppState>) -> Result<CampaignContext, ApiError> {
    let slug = extract_slug_from_path(path)?;
    let campaign = CampaignService::resolve_by_slug(state, &slug).await?;
    Ok(CampaignContext {
        paths: CampaignPaths::new(campaign.id.clone()),
        campaign,
    })
}

fn extract_slug_from_path(path: &str) -> ApiResult<String> {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if let Some(pos) = segments.iter().position(|&s| s == "campaigns") {
        if let Some(slug) = segments.get(pos + 1) {
            if *slug != "csrf-token" && !slug.is_empty() {
                return Ok(slug.to_string());
            }
        }
    }
    Err(ApiError::bad_request("Campaign slug missing from path."))
}

#[derive(serde::Deserialize)]
pub struct SlugPath {
    pub slug: String,
}

impl SlugPath {
    pub async fn into_context(self, state: &AppState) -> ApiResult<CampaignContext> {
        let campaign = CampaignService::resolve_by_slug(state, &self.slug).await?;
        Ok(CampaignContext {
            paths: CampaignPaths::new(campaign.id.clone()),
            campaign,
        })
    }
}
