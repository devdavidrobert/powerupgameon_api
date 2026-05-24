use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::features::campaigns::domain::Campaign;
use crate::features::campaigns::infrastructure::CampaignPaths;
use crate::features::campaigns::presentation::CampaignContext;
use crate::features::inventory::application::InventoryService;
use crate::features::inventory::domain::InventorySlot;
use crate::features::inventory::infrastructure::InventoryRepository;
use crate::features::spin::domain::{
    is_real_prize, partition_spin_pool, prize_id_from_map, spin_prize_from_entry,
    ClaimableRealEntry, SpinPoolEntry, SpinPrize, SpinResult,
};
use crate::logger;
use crate::middleware::request_context::RequestContext;
use axum::Json;
use rand::Rng;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

pub const MAX_CLAIM_RETRIES: usize = 8;
pub const REAL_BIAS_FACTOR: f64 = 2.0;
pub const MIN_REAL_WEIGHT: f64 = 0.01;
pub const CONSOLATION_BIAS_FACTOR: f64 = 3.0;

#[derive(Debug, Clone)]
pub struct SchedulePressureMetrics {
    pub total_releasable_budget: i64,
    pub total_awarded: i64,
    pub total_remaining: i64,
    pub campaign_elapsed: f64,
    pub schedule_pressure: f64,
    pub deficit: f64,
    pub real_weight: f64,
    pub consolation_weight: f64,
}

#[derive(Debug, Clone)]
pub struct LocationPoolSnapshot {
    pub real_claimable: Vec<ClaimableRealEntry>,
    pub consolation: Vec<SpinPoolEntry>,
    pub metrics: SchedulePressureMetrics,
}

pub struct SpinService;

impl SpinService {
    pub fn campaign_elapsed(campaign: &Campaign, now_ms: i64) -> f64 {
        let Some(start) = campaign.challenge_start_time else {
            return 1.0;
        };
        let Some(end) = campaign.challenge_end_time else {
            return 1.0;
        };
        if end <= start {
            return 1.0;
        }
        let clamped = now_ms.clamp(start, end);
        (clamped - start) as f64 / (end - start) as f64
    }

    pub fn compute_schedule_pressure_metrics(
        campaign: &Campaign,
        real_slots: &[&InventorySlot],
        real_claimable: &[ClaimableRealEntry],
        now_ms: i64,
    ) -> SchedulePressureMetrics {
        let mut total_releasable_budget = 0_i64;
        let mut total_awarded = 0_i64;

        for slot in real_slots {
            let releasable = InventoryService::releasable_now(campaign, slot, now_ms);
            total_releasable_budget += releasable;
            total_awarded += slot.awarded_count;
        }

        let total_remaining: i64 = real_claimable.iter().map(|e| e.remaining).sum();
        let campaign_elapsed = Self::campaign_elapsed(campaign, now_ms);
        let schedule_pressure = total_awarded as f64 / total_releasable_budget.max(1) as f64;
        let deficit = campaign_elapsed - schedule_pressure;

        let real_weight =
            (total_remaining as f64 * (1.0 + deficit * REAL_BIAS_FACTOR)).max(MIN_REAL_WEIGHT);
        let consolation_weight = 1.0 + (-deficit).max(0.0) * CONSOLATION_BIAS_FACTOR;

        SchedulePressureMetrics {
            total_releasable_budget,
            total_awarded,
            total_remaining,
            campaign_elapsed,
            schedule_pressure,
            deficit,
            real_weight,
            consolation_weight,
        }
    }

    pub fn build_location_pool_snapshot(
        campaign: &Campaign,
        slot_by_prize: &HashMap<String, InventorySlot>,
        prizes: &[Map<String, Value>],
        now_ms: i64,
        excluded_real_ids: &HashSet<String>,
    ) -> LocationPoolSnapshot {
        let (real_claimable, consolation) =
            partition_spin_pool(prizes, slot_by_prize, campaign, now_ms, excluded_real_ids);

        let real_slots: Vec<&InventorySlot> = real_claimable
            .iter()
            .filter_map(|e| slot_by_prize.get(&e.prize_id))
            .collect();

        let metrics =
            Self::compute_schedule_pressure_metrics(campaign, &real_slots, &real_claimable, now_ms);

        LocationPoolSnapshot {
            real_claimable,
            consolation,
            metrics,
        }
    }

    pub fn select_entry(snapshot: &LocationPoolSnapshot, rng: &mut impl Rng) -> SpinPoolEntry {
        if snapshot.real_claimable.is_empty() {
            return Self::pick_consolation_uniform(&snapshot.consolation, rng);
        }

        let real_weight = snapshot.metrics.real_weight;
        let consolation_weight = snapshot.metrics.consolation_weight;
        let total = real_weight + consolation_weight;

        if rng.gen::<f64>() * total < real_weight {
            Self::pick_real_weighted(&snapshot.real_claimable, rng)
        } else {
            Self::pick_consolation_uniform(&snapshot.consolation, rng)
        }
    }

    pub fn pick_real_weighted(entries: &[ClaimableRealEntry], rng: &mut impl Rng) -> SpinPoolEntry {
        let total: i64 = entries.iter().map(|e| e.remaining).sum();
        if total <= 0 {
            let first = &entries[0];
            return (first.prize.clone(), first.prize_id.clone());
        }

        let mut pick = rng.gen_range(0..total);
        for entry in entries {
            if pick < entry.remaining {
                return (entry.prize.clone(), entry.prize_id.clone());
            }
            pick -= entry.remaining;
        }

        let last = entries.last().expect("entries non-empty");
        (last.prize.clone(), last.prize_id.clone())
    }

    pub fn pick_consolation_uniform(
        consolation: &[SpinPoolEntry],
        rng: &mut impl Rng,
    ) -> SpinPoolEntry {
        if consolation.is_empty() {
            return (Map::new(), String::new());
        }
        let idx = rng.gen_range(0..consolation.len());
        consolation[idx].clone()
    }

    pub async fn build_pool_snapshot_for_location(
        state: &AppState,
        paths: &CampaignPaths,
        campaign: &Campaign,
        location_id: &str,
        prizes: &[Map<String, Value>],
        excluded_real_ids: &HashSet<String>,
    ) -> ApiResult<LocationPoolSnapshot> {
        let slots = InventoryRepository::find_by_location(state, paths, location_id).await?;
        let now = chrono::Utc::now().timestamp_millis();

        let slot_by_prize: HashMap<String, InventorySlot> =
            slots.into_iter().map(|s| (s.prize_id.clone(), s)).collect();

        Ok(Self::build_location_pool_snapshot(
            campaign,
            &slot_by_prize,
            prizes,
            now,
            excluded_real_ids,
        ))
    }

    pub async fn finalize_spin_claim(
        state: &AppState,
        ctx: &CampaignContext,
        req_ctx: &RequestContext,
        session_id: &str,
        location_id: &str,
        sorted: &[Map<String, Value>],
        fingerprint: &str,
        entry: SpinPoolEntry,
        award_as_real: bool,
        attempt: Option<usize>,
    ) -> ApiResult<Json<SuccessResponse<Value>>> {
        if entry.1.is_empty() {
            return Err(ApiError::bad_request("No prizes available to spin."));
        }

        let (prize_id, prize_name, order, is_real) = spin_prize_from_entry(&entry, award_as_real);

        match InventoryRepository::claim_atomic(
            state,
            &ctx.paths,
            &ctx.campaign,
            session_id,
            location_id,
            &prize_id,
            &prize_name,
            is_real,
            fingerprint,
        )
        .await
        {
            Ok(result) if result.finalized => {
                if let Some(n) = attempt {
                    logger::log(
                        &state.config,
                        "info",
                        "spin_audit",
                        json!({
                            "requestId": req_ctx.request_id,
                            "sessionIdPrefix": &session_id[..session_id.len().min(10)],
                            "campaignSlug": ctx.slug(),
                            "prize": prize_name,
                            "isRealPrize": is_real,
                            "attempt": n,
                            "tokenFingerprint": &fingerprint[..fingerprint.len().min(16)],
                        }),
                    );
                }
                Ok(SuccessResponse::data(
                    SpinResult {
                        campaign_slug: ctx.slug().to_string(),
                        prize: SpinPrize {
                            id: prize_id,
                            name: prize_name,
                            order,
                            is_real_prize: is_real,
                        },
                        idempotent: None,
                    }
                    .to_json(),
                ))
            }
            Ok(result) => {
                if let Some(prev_name) = result.previous_prize {
                    let prev = sorted.iter().find(|p| {
                        p.get("name").and_then(|v| v.as_str()) == Some(prev_name.as_str())
                    });
                    let prev_id = prev.and_then(prize_id_from_map).unwrap_or_default();
                    let prev_order = prev
                        .and_then(|p| p.get("order").and_then(|v| v.as_i64()))
                        .unwrap_or(order);
                    Ok(SuccessResponse::data(
                        SpinResult {
                            campaign_slug: ctx.slug().to_string(),
                            prize: SpinPrize {
                                id: prev_id,
                                name: prev_name,
                                order: prev_order,
                                is_real_prize: prev.map(is_real_prize).unwrap_or(false),
                            },
                            idempotent: Some(true),
                        }
                        .to_json(),
                    ))
                } else {
                    Err(ApiError::bad_request("Spin could not be finalized."))
                }
            }
            Err(err) => Err(err),
        }
    }
}
