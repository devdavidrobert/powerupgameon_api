use crate::app_state::AppState;
use crate::error::ApiResult;
use crate::features::campaigns::application::CampaignService;
use crate::features::campaigns::infrastructure::CampaignPaths;
use crate::features::campaigns::presentation::{CampaignContext, SlugPath};
use axum::{
    extract::{Path, Request, State},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

/// Resolves the campaign slug from the nested `{slug}` path param and stores `CampaignContext`.
/// Must be applied on the `/api/campaigns/{slug}` nest, not on inner sub-routers.
pub async fn inject_campaign_context(
    State(state): State<Arc<AppState>>,
    Path(slug_path): Path<SlugPath>,
    mut req: Request,
    next: Next,
) -> ApiResult<Response> {
    if req.extensions().get::<CampaignContext>().is_none() {
        let campaign = CampaignService::resolve_by_slug(&state, &slug_path.slug).await?;
        req.extensions_mut().insert(CampaignContext {
            paths: CampaignPaths::new(campaign.id.clone()),
            campaign,
        });
    }
    Ok(next.run(req).await)
}
