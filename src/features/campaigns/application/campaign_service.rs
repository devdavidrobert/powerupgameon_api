use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::domain::Campaign;
use crate::features::campaigns::infrastructure::CampaignRepository;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Clone)]
struct CachedCampaign {
    campaign: Campaign,
    cached_at: Instant,
}

static SLUG_CACHE: Lazy<DashMap<String, CachedCampaign>> = Lazy::new(DashMap::new);

pub struct CampaignService;

impl CampaignService {
    pub fn invalidate_slug(slug: &str) {
        SLUG_CACHE.remove(slug);
    }

    pub fn invalidate_all() {
        SLUG_CACHE.clear();
    }

    pub async fn resolve_by_slug(state: &AppState, slug: &str) -> ApiResult<Campaign> {
        if let Some(entry) = SLUG_CACHE.get(slug) {
            if entry.cached_at.elapsed() < CACHE_TTL {
                return Ok(entry.campaign.clone());
            }
            SLUG_CACHE.remove(slug);
        }

        let campaign = CampaignRepository::find_by_slug(state, slug)
            .await?
            .ok_or_else(|| {
                ApiError::with_code(
                    axum::http::StatusCode::NOT_FOUND,
                    "CAMPAIGN_NOT_FOUND",
                    "Campaign not found.",
                )
            })?;

        SLUG_CACHE.insert(
            slug.to_string(),
            CachedCampaign {
                campaign: campaign.clone(),
                cached_at: Instant::now(),
            },
        );

        Ok(campaign)
    }
}
