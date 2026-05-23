use crate::app_state::AppState;
use crate::error::ApiResult;
use crate::features::campaigns::application::CampaignService;
use crate::features::campaigns::infrastructure::CampaignPaths;
use crate::features::campaigns::presentation::{resolve_campaign_slug, CampaignContext};
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

/// Resolves the campaign slug from the request path and stores `CampaignContext`.
/// Handlers also fall back to path parsing when this middleware did not run.
pub async fn inject_campaign_context(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> ApiResult<Response> {
    if req.extensions().get::<CampaignContext>().is_none() {
        let slug = resolve_campaign_slug(req.uri().path(), req.headers())?;
        let campaign = CampaignService::resolve_by_slug(&state, &slug).await?;
        req.extensions_mut().insert(CampaignContext {
            paths: CampaignPaths::new(campaign.id.clone()),
            campaign,
        });
    }
    Ok(next.run(req).await)
}
