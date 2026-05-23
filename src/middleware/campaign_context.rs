use crate::app_state::AppState;
use crate::error::ApiResult;
use crate::features::campaigns::application::CampaignService;
use crate::features::campaigns::infrastructure::CampaignPaths;
use crate::features::campaigns::presentation::{CampaignContext, SlugPath};
use axum::{
    extract::{Path, State},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

/// Resolves `{slug}` from Axum route params and stores `CampaignContext` on the request.
/// Required on Vercel: `uri.path()` is rewritten to `/api/main`, so parsing the URI fails.
pub async fn inject_campaign_context(
    State(state): State<Arc<AppState>>,
    Path(slug_path): Path<SlugPath>,
    mut req: axum::extract::Request,
    next: Next,
) -> ApiResult<Response> {
    let campaign = CampaignService::resolve_by_slug(&state, &slug_path.slug).await?;
    req.extensions_mut().insert(CampaignContext {
        paths: CampaignPaths::new(campaign.id.clone()),
        campaign,
    });
    Ok(next.run(req).await)
}
