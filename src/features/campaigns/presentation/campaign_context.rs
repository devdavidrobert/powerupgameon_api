use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::application::CampaignService;
use crate::features::campaigns::domain::Campaign;
use crate::features::campaigns::infrastructure::CampaignPaths;
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header::HeaderMap, request::Parts, StatusCode},
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
        let cached = parts.extensions.get::<CampaignContext>().cloned();
        let path = parts.uri.path().to_string();
        let query = parts.uri.query().map(str::to_string);
        let headers = parts.headers.clone();
        let state = Arc::<AppState>::from_ref(state);

        async move {
            let ctx = if let Some(ctx) = cached {
                ctx
            } else {
                load_campaign_context(&path, query.as_deref(), &headers, &state).await?
            };

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
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let cached = parts.extensions.get::<CampaignContext>().cloned();
        let path = parts.uri.path().to_string();
        let query = parts.uri.query().map(str::to_string);
        let headers = parts.headers.clone();
        let state = Arc::<AppState>::from_ref(state);

        async move {
            if let Some(ctx) = cached {
                Ok(ctx)
            } else {
                load_campaign_context(&path, query.as_deref(), &headers, &state).await
            }
        }
    }
}

pub fn extract_slug_from_path(path: &str) -> ApiResult<String> {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if let Some(pos) = segments.iter().position(|&s| s == "campaigns") {
        if let Some(slug) = segments.get(pos + 1) {
            if *slug != "csrf-token" && !slug.is_empty() {
                return Ok(slug.to_string());
            }
        }
    }
    Err(campaign_required_error())
}

fn campaign_required_error() -> ApiError {
    ApiError::with_code(
        StatusCode::BAD_REQUEST,
        "CAMPAIGN_REQUIRED",
        "You must choose a campaign to play.",
    )
}

pub fn resolve_campaign_slug(path: &str, query: Option<&str>, headers: &HeaderMap) -> ApiResult<String> {
    if let Ok(slug) = extract_slug_from_path(path) {
        return Ok(slug);
    }

    if let Some(restored) = crate::middleware::vercel_path::resolve_original_path(query, headers) {
        if let Ok(slug) = extract_slug_from_path(&restored) {
            return Ok(slug);
        }
    }

    for header in ["x-invoke-path", "x-original-url", "x-forwarded-uri"] {
        if let Some(value) = headers.get(header).and_then(|v| v.to_str().ok()) {
            let path_only = value.split('?').next().unwrap_or(value);
            if let Ok(slug) = extract_slug_from_path(path_only) {
                return Ok(slug);
            }
        }
    }

    Err(campaign_required_error())
}

async fn load_campaign_context(
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    state: &Arc<AppState>,
) -> Result<CampaignContext, ApiError> {
    let slug = resolve_campaign_slug(path, query, headers)?;
    let campaign = CampaignService::resolve_by_slug(state, &slug).await?;
    Ok(CampaignContext {
        paths: CampaignPaths::new(campaign.id.clone()),
        campaign,
    })
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
