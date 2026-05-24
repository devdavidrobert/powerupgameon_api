use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::features::campaigns::presentation::PublicCampaignContext;
use crate::features::inventory::application::InventoryService;
use crate::features::inventory::domain::InventorySlot;
use crate::features::inventory::infrastructure::InventoryRepository;
use crate::logger;
use crate::middleware::request_context::RequestContext;
use crate::models::prize::PrizeModel;
use crate::models::registration::RegistrationModel;
use crate::models::submission::SubmissionModel;
use crate::utils::firestore::document_id_from_map;
use crate::utils::spin_token::verify_spin_token;
use axum::{extract::State, Extension, Json};
use rand::Rng;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

const DEFAULT_ANIMATION_MS: i64 = 5000;
const MAX_CLAIM_RETRIES: usize = 8;

type SpinPoolEntry = (serde_json::Map<String, Value>, String);

#[derive(Deserialize)]
pub struct SpinBody {
    #[serde(rename = "spinToken")]
    pub spin_token: Option<String>,
}

pub async fn spin_wheel(
    State(state): State<Arc<AppState>>,
    PublicCampaignContext(ctx): PublicCampaignContext,
    Extension(req_ctx): Extension<RequestContext>,
    Json(body): Json<SpinBody>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    let spin_token = body
        .spin_token
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ApiError::with_code(
                axum::http::StatusCode::BAD_REQUEST,
                "SPIN_TOKEN_REQUIRED",
                "spinToken is required. Complete the quiz to receive a token.",
            )
        })?;

    let (session_id, token_campaign_id) = verify_spin_token(&state.config, spin_token)?;
    if token_campaign_id != ctx.campaign.id {
        return Err(ApiError::with_code(
            axum::http::StatusCode::BAD_REQUEST,
            "SPIN_TOKEN_INVALID",
            "Spin token does not match this campaign.",
        ));
    }

    let fingerprint = hex::encode(Sha256::digest(spin_token.as_bytes()));

    let existing = SubmissionModel::find_by_id(&state, &ctx.paths, &session_id)
        .await?
        .ok_or_else(|| ApiError::bad_request("Submission not found for this session."))?;

    if existing.get("prize").and_then(|v| v.as_str()) != Some("pending") {
        let prize_name = existing.get("prize").and_then(|v| v.as_str()).unwrap_or("");
        let prizes = PrizeModel::find_all(&state, &ctx.paths).await?;
        let won = prizes
            .iter()
            .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(prize_name));
        let order = won
            .and_then(|p| p.get("order").and_then(|v| v.as_i64()))
            .unwrap_or(0);
        let is_real = won.map(is_real_prize).unwrap_or(false);
        return Ok(SuccessResponse::data(json!({
            "prize": { "name": prize_name, "order": order, "isRealPrize": is_real },
            "animationDuration": DEFAULT_ANIMATION_MS,
            "idempotent": true,
        })));
    }

    let registration = RegistrationModel::find_by_id(&state, &ctx.paths, &session_id)
        .await?
        .ok_or_else(|| ApiError::bad_request("Registration not found for this session."))?;

    if registration.get("geoStatus").and_then(|v| v.as_str()) == Some("outside") {
        return Err(ApiError::with_code(
            axum::http::StatusCode::FORBIDDEN,
            "GEO_OUTSIDE_ZONE",
            "Real prizes are not available outside allowed zones.",
        ));
    }

    let location_id = registration
        .get("locationId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ApiError::bad_request("Registration is missing location context for prize allocation.")
        })?
        .to_string();

    let prizes = PrizeModel::find_all(&state, &ctx.paths).await?;
    if prizes.is_empty() {
        logger::log(
            &state.config,
            "error",
            "spin_no_prizes",
            json!({ "requestId": req_ctx.request_id }),
        );
        return Err(ApiError::bad_request("No prizes configured."));
    }

    let mut sorted = prizes.clone();
    sorted.sort_by_key(|p| p.get("order").and_then(|v| v.as_i64()).unwrap_or(0));

    let mut excluded_real_ids: HashSet<String> = HashSet::new();
    let mut last_error = ApiError::bad_request("No prizes available to spin.");

    for attempt in 0..MAX_CLAIM_RETRIES {
        let pool = build_spin_pool(
            &state,
            &ctx.paths,
            &ctx.campaign,
            &location_id,
            &sorted,
            &excluded_real_ids,
        )
        .await?;

        if pool.is_empty() {
            break;
        }

        let pick_idx = rand::thread_rng().gen_range(0..pool.len());
        let entry = pool[pick_idx].clone();
        let is_real = is_real_prize(&entry.0);

        match finalize_spin_claim(
            &state,
            &ctx,
            &req_ctx,
            &session_id,
            &location_id,
            &sorted,
            &fingerprint,
            entry.clone(),
            is_real,
            Some(attempt + 1),
        )
        .await
        {
            Ok(response) => return Ok(response),
            Err(ApiError::WithStatus { code: Some(code), .. }) if code == "INVENTORY_EXHAUSTED" => {
                if is_real {
                    excluded_real_ids.insert(entry.1);
                }
                last_error = ApiError::bad_request("Prize inventory exhausted.");
                continue;
            }
            Err(ApiError::WithStatus { code: Some(code), .. }) if code == "SPIN_TOKEN_ALREADY_USED" => {
                logger::log(
                    &state.config,
                    "warn",
                    "spin_token_replay_attempt",
                    json!({
                        "requestId": req_ctx.request_id,
                        "sessionIdPrefix": &session_id[..session_id.len().min(10)],
                    }),
                );
                return Err(ApiError::with_code(
                    axum::http::StatusCode::CONFLICT,
                    "SPIN_TOKEN_ALREADY_USED",
                    "This spin token has already been used.",
                ));
            }
            Err(err) => return Err(err),
        }
    }

    if let Some(entry) = pick_wheel_fallback(&sorted) {
        return finalize_spin_claim(
            &state,
            &ctx,
            &req_ctx,
            &session_id,
            &location_id,
            &sorted,
            &fingerprint,
            entry,
            false,
            None,
        )
        .await;
    }

    Err(last_error)
}

async fn finalize_spin_claim(
    state: &AppState,
    ctx: &crate::features::campaigns::presentation::CampaignContext,
    req_ctx: &RequestContext,
    session_id: &str,
    location_id: &str,
    sorted: &[serde_json::Map<String, Value>],
    fingerprint: &str,
    entry: SpinPoolEntry,
    award_as_real: bool,
    attempt: Option<usize>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    let (won, prize_id) = entry;
    let prize_name = won.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let order = won.get("order").and_then(|v| v.as_i64()).unwrap_or(0);
    let is_real = award_as_real && is_real_prize(&won);

    match InventoryRepository::claim_atomic(
        state,
        &ctx.paths,
        &ctx.campaign,
        session_id,
        location_id,
        &prize_id,
        prize_name,
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
            Ok(SuccessResponse::data(json!({
                "prize": {
                    "name": prize_name,
                    "order": order,
                    "isRealPrize": is_real,
                },
                "animationDuration": DEFAULT_ANIMATION_MS,
            })))
        }
        Ok(result) => {
            if let Some(prev_name) = result.previous_prize {
                let prev = sorted
                    .iter()
                    .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(prev_name.as_str()));
                Ok(SuccessResponse::data(json!({
                    "prize": {
                        "name": prev_name,
                        "order": prev.and_then(|p| p.get("order").and_then(|v| v.as_i64())).unwrap_or(order),
                        "isRealPrize": prev.map(is_real_prize).unwrap_or(false),
                    },
                    "animationDuration": DEFAULT_ANIMATION_MS,
                    "idempotent": true,
                })))
            } else {
                Err(ApiError::bad_request("Spin could not be finalized."))
            }
        }
        Err(err) => Err(err),
    }
}

async fn build_spin_pool(
    state: &AppState,
    paths: &crate::features::campaigns::infrastructure::CampaignPaths,
    campaign: &crate::features::campaigns::domain::Campaign,
    location_id: &str,
    prizes: &[serde_json::Map<String, Value>],
    excluded_real_ids: &HashSet<String>,
) -> ApiResult<Vec<SpinPoolEntry>> {
    let slots = InventoryRepository::find_by_location(state, paths, location_id).await?;
    let now = chrono::Utc::now().timestamp_millis();

    let slot_by_prize: HashMap<String, InventorySlot> = slots
        .into_iter()
        .map(|s| (s.prize_id.clone(), s))
        .collect();

    let (mut real_claimable, consolation) =
        partition_spin_pool(prizes, &slot_by_prize, campaign, now, excluded_real_ids);

    if !real_claimable.is_empty() {
        real_claimable.extend(consolation);
        return Ok(real_claimable);
    }

    Ok(consolation)
}

fn partition_spin_pool(
    prizes: &[serde_json::Map<String, Value>],
    slot_by_prize: &HashMap<String, InventorySlot>,
    campaign: &crate::features::campaigns::domain::Campaign,
    now: i64,
    excluded_real_ids: &HashSet<String>,
) -> (Vec<SpinPoolEntry>, Vec<SpinPoolEntry>) {
    let mut real_claimable = Vec::new();
    let mut consolation = Vec::new();

    for prize in prizes {
        let Some(prize_id) = prize_id_from_map(prize) else {
            continue;
        };

        if is_consolation_prize(prize) {
            consolation.push((prize.clone(), prize_id));
            continue;
        }

        if excluded_real_ids.contains(&prize_id) {
            continue;
        }

        if let Some(slot) = slot_by_prize.get(&prize_id) {
            let releasable = InventoryService::releasable_now(campaign, slot, now);
            if slot.is_claimable(releasable) {
                real_claimable.push((prize.clone(), prize_id));
            }
        }
    }

    (real_claimable, consolation)
}

fn prize_id_from_map(prize: &serde_json::Map<String, Value>) -> Option<String> {
    prize
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| document_id_from_map(prize))
}

fn pick_wheel_fallback(prizes: &[serde_json::Map<String, Value>]) -> Option<SpinPoolEntry> {
    for prize in prizes {
        if is_consolation_prize(prize) {
            if let Some(entry) = prize_entry(prize) {
                return Some(entry);
            }
        }
    }

    prizes.iter().rev().filter_map(prize_entry).next()
}

fn prize_entry(prize: &serde_json::Map<String, Value>) -> Option<SpinPoolEntry> {
    let id = prize_id_from_map(prize)?;
    Some((prize.clone(), id))
}

fn is_consolation_prize(prize: &serde_json::Map<String, Value>) -> bool {
    if prize.get("isRealPrize").and_then(|v| v.as_bool()) == Some(false) {
        return true;
    }

    let name = prize
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_lowercase();

    name.contains("so close")
        || name.contains("try again")
        || name.contains("better luck")
        || name == "👋"
        || name == "😢"
}

fn is_real_prize(prize: &serde_json::Map<String, Value>) -> bool {
    !is_consolation_prize(prize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::campaigns::domain::{Campaign, CampaignStatus, GeoEnforcement, StaggerMode};
    use serde_json::Map;
    use std::collections::HashMap;

    fn sample_campaign() -> Campaign {
        Campaign {
            id: "camp-1".into(),
            slug: "test".into(),
            name: "Test".into(),
            status: CampaignStatus::Active,
            challenge_start_time: None,
            challenge_end_time: None,
            stagger_mode: StaggerMode::Immediate,
            stagger_schedule: None,
            geo_enforcement: GeoEnforcement::Reject,
            created_at: None,
            updated_at: None,
        }
    }

    fn prize(name: &str, id: &str, order: i64, is_real: bool) -> serde_json::Map<String, Value> {
        Map::from_iter([
            ("name".into(), json!(name)),
            ("id".into(), json!(id)),
            ("order".into(), json!(order)),
            ("isRealPrize".into(), json!(is_real)),
        ])
    }

    #[test]
    fn partition_includes_consolation_when_real_inventory_is_exhausted() {
        let prizes = vec![
            prize("Steam Can", "p1", 1, true),
            prize("So Close", "p2", 2, false),
        ];
        let slot = InventorySlot {
            id: "loc_p1".into(),
            location_id: "loc".into(),
            prize_id: "p1".into(),
            total_quantity: 1,
            awarded_count: 1,
            updated_at: None,
        };
        let slots = HashMap::from([("p1".into(), slot)]);
        let excluded = HashSet::new();

        let (real, consolation) =
            partition_spin_pool(&prizes, &slots, &sample_campaign(), 0, &excluded);

        assert!(real.is_empty());
        assert_eq!(consolation.len(), 1);
        assert_eq!(consolation[0].0.get("name").and_then(|v| v.as_str()), Some("So Close"));
    }

    #[test]
    fn partition_mixes_claimable_real_and_consolation() {
        let prizes = vec![
            prize("Steam Can", "p1", 1, true),
            prize("So Close", "p2", 2, false),
        ];
        let slot = InventorySlot {
            id: "loc_p1".into(),
            location_id: "loc".into(),
            prize_id: "p1".into(),
            total_quantity: 5,
            awarded_count: 0,
            updated_at: None,
        };
        let slots = HashMap::from([("p1".into(), slot)]);
        let excluded = HashSet::new();

        let (real, consolation) =
            partition_spin_pool(&prizes, &slots, &sample_campaign(), 0, &excluded);

        assert_eq!(real.len(), 1);
        assert_eq!(consolation.len(), 1);
    }

    #[test]
    fn so_close_is_consolation_even_without_flag() {
        let mut p = prize("So Close", "p2", 2, true);
        p.remove("isRealPrize");
        assert!(is_consolation_prize(&p));
    }

    #[test]
    fn pick_wheel_fallback_uses_last_prize_when_no_consolation() {
        let prizes = vec![prize("Steam Can", "p1", 1, true), prize("Merch", "p2", 2, true)];
        let picked = pick_wheel_fallback(&prizes).expect("fallback");
        assert_eq!(picked.0.get("name").and_then(|v| v.as_str()), Some("Merch"));
    }
}
