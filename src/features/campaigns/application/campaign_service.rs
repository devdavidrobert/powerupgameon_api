use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::domain::Campaign;
use crate::features::campaigns::infrastructure::CampaignRepository;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(60);
const MAX_CACHE_ENTRIES: usize = 500;

#[derive(Clone)]
struct CachedCampaign {
    campaign: Campaign,
    cached_at: Instant,
}

static SLUG_CACHE: Lazy<DashMap<String, CachedCampaign>> = Lazy::new(DashMap::new);

fn evict_stale_slug_cache_entries() {
    SLUG_CACHE.retain(|_, entry| entry.cached_at.elapsed() < CACHE_TTL);
}

fn enforce_slug_cache_max_size() {
    if SLUG_CACHE.len() <= MAX_CACHE_ENTRIES {
        return;
    }
    let mut entries: Vec<(String, Instant)> = SLUG_CACHE
        .iter()
        .map(|entry| (entry.key().clone(), entry.cached_at))
        .collect();
    entries.sort_by_key(|(_, cached_at)| *cached_at);
    let to_remove = SLUG_CACHE.len().saturating_sub(MAX_CACHE_ENTRIES);
    for (slug, _) in entries.into_iter().take(to_remove) {
        SLUG_CACHE.remove(&slug);
    }
}

pub struct CampaignService;

impl CampaignService {
    pub fn invalidate_slug(slug: &str) {
        SLUG_CACHE.remove(slug);
    }

    pub fn invalidate_all() {
        SLUG_CACHE.clear();
    }

    pub async fn resolve_by_slug(state: &AppState, slug: &str) -> ApiResult<Campaign> {
        evict_stale_slug_cache_entries();

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
        enforce_slug_cache_max_size();

        Ok(campaign)
    }
}
