use crate::error::ApiResult;
use crate::features::campaigns::presentation::CampaignContext;
use crate::utils::challenge_window::assert_challenge_open_for_campaign;
use axum::{extract::Request, middleware::Next, response::Response};

/// Blocks public play endpoints when the campaign challenge window is closed.
/// Must run after `inject_campaign_context`.
pub async fn require_challenge_open_middleware(req: Request, next: Next) -> ApiResult<Response> {
    if let Some(ctx) = req.extensions().get::<CampaignContext>() {
        assert_challenge_open_for_campaign(&ctx.campaign)?;
    }
    Ok(next.run(req).await)
}
