use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::features::campaigns::presentation::PublicCampaignContext;
use crate::features::inventory::application::InventoryService;
use crate::features::inventory::infrastructure::InventoryRepository;
use crate::logger;
use crate::middleware::request_context::RequestContext;
use crate::models::prize::PrizeModel;
use crate::models::registration::RegistrationModel;
use crate::models::submission::SubmissionModel;
use crate::utils::spin_token::verify_spin_token;
use axum::{extract::State, Extension, Json};
use rand::Rng;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;

const DEFAULT_ANIMATION_MS: i64 = 5000;
const MAX_CLAIM_RETRIES: usize = 3;

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
        let is_real = won
            .and_then(|p| p.get("isRealPrize").and_then(|v| v.as_bool()))
            .unwrap_or(false);
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

    let mut last_error = ApiError::bad_request("Prize inventory exhausted.");

    for attempt in 0..MAX_CLAIM_RETRIES {
        let pool = build_spin_pool(
            &state,
            &ctx.paths,
            &ctx.campaign,
            &location_id,
            &sorted,
        )
        .await?;

        if pool.is_empty() {
            return Err(ApiError::bad_request("Prize inventory exhausted."));
        }

        let pick_idx = rand::thread_rng().gen_range(0..pool.len());
        let (won, prize_id) = &pool[pick_idx];
        let prize_name = won.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let order = won.get("order").and_then(|v| v.as_i64()).unwrap_or(0);
        let is_real = won
            .get("isRealPrize")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        match InventoryRepository::claim_atomic(
            &state,
            &ctx.paths,
            &ctx.campaign,
            &session_id,
            &location_id,
            prize_id,
            prize_name,
            is_real,
            &fingerprint,
        )
        .await
        {
            Ok(result) if result.finalized => {
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
                        "attempt": attempt + 1,
                        "tokenFingerprint": &fingerprint[..fingerprint.len().min(16)],
                    }),
                );
                return Ok(SuccessResponse::data(json!({
                    "prize": {
                        "name": prize_name,
                        "order": order,
                        "isRealPrize": is_real,
                    },
                    "animationDuration": DEFAULT_ANIMATION_MS,
                })));
            }
            Ok(result) => {
                if let Some(prev_name) = result.previous_prize {
                    let prev = sorted
                        .iter()
                        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(prev_name.as_str()));
                    return Ok(SuccessResponse::data(json!({
                        "prize": {
                            "name": prev_name,
                            "order": prev.and_then(|p| p.get("order").and_then(|v| v.as_i64())).unwrap_or(order),
                            "isRealPrize": prev.and_then(|p| p.get("isRealPrize").and_then(|v| v.as_bool())).unwrap_or(false),
                        },
                        "animationDuration": DEFAULT_ANIMATION_MS,
                        "idempotent": true,
                    })));
                }
            }
            Err(ApiError::WithStatus { code: Some(code), .. }) if code == "INVENTORY_EXHAUSTED" => {
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

    Err(last_error)
}

async fn build_spin_pool(
    state: &AppState,
    paths: &crate::features::campaigns::infrastructure::CampaignPaths,
    campaign: &crate::features::campaigns::domain::Campaign,
    location_id: &str,
    prizes: &[serde_json::Map<String, Value>],
) -> ApiResult<Vec<(serde_json::Map<String, Value>, String)>> {
    let slots = InventoryRepository::find_by_location(state, paths, location_id).await?;
    let now = chrono::Utc::now().timestamp_millis();

    let slot_by_prize: HashMap<String, _> = slots
        .into_iter()
        .map(|s| (s.prize_id.clone(), s))
        .collect();

    let mut available = Vec::new();
    for prize in prizes {
        let prize_id = prize
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("Prize missing id")))?
            .to_string();
        let is_real = prize.get("isRealPrize").and_then(|v| v.as_bool()).unwrap_or(true);
        if !is_real {
            available.push((prize.clone(), prize_id));
            continue;
        }
        if let Some(slot) = slot_by_prize.get(&prize_id) {
            let releasable = InventoryService::releasable_now(campaign, slot, now);
            if slot.is_claimable(releasable) {
                available.push((prize.clone(), prize_id));
            }
        }
    }

    if !available.is_empty() {
        return Ok(available);
    }

    Ok(prizes
        .iter()
        .filter(|p| !p.get("isRealPrize").and_then(|v| v.as_bool()).unwrap_or(true))
        .filter_map(|p| {
            let id = p.get("id")?.as_str()?.to_string();
            Some((p.clone(), id))
        })
        .collect())
}
