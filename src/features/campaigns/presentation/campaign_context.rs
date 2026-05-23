use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::application::CampaignService;
use crate::features::campaigns::domain::Campaign;
use crate::features::campaigns::infrastructure::CampaignPaths;
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{request::Parts, StatusCode},
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
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        async move {
            let ctx = campaign_context_from_parts(parts)?;
            if !ctx.campaign.is_publicly_accessible() {
                return Err(ApiError::with_code(
                    StatusCode::FORBIDDEN,
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
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        async move { campaign_context_from_parts(parts) }
    }
}

fn campaign_context_from_parts(parts: &Parts) -> Result<CampaignContext, ApiError> {
    parts
        .extensions
        .get::<CampaignContext>()
        .cloned()
        .ok_or_else(|| ApiError::bad_request("Campaign slug missing from path."))
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
