use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::features::campaigns::presentation::PublicCampaignContext;
use crate::features::spin::application::SpinService;
use crate::features::spin::domain::{
    is_real_prize, pick_wheel_fallback, prize_id_from_map, SpinPrize, SpinResult,
};
use crate::logger;
use crate::middleware::request_context::RequestContext;
use crate::models::prize::PrizeModel;
use crate::models::registration::RegistrationModel;
use crate::models::submission::SubmissionModel;
use crate::utils::spin_token::verify_spin_token;
use axum::{extract::State, Extension, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::Arc;

use crate::features::spin::application::MAX_CLAIM_RETRIES;

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
        let prize_id = won
            .and_then(prize_id_from_map)
            .unwrap_or_default();
        let order = won
            .and_then(|p| p.get("order").and_then(|v| v.as_i64()))
            .unwrap_or(0);
        let is_real = won.map(is_real_prize).unwrap_or(false);
        return Ok(SuccessResponse::data(
            SpinResult {
                campaign_slug: ctx.slug().to_string(),
                prize: SpinPrize {
                    id: prize_id,
                    name: prize_name.to_string(),
                    order,
                    is_real_prize: is_real,
                },
                idempotent: Some(true),
            }
            .to_json(),
        ));
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
        let snapshot = SpinService::build_pool_snapshot_for_location(
            &state,
            &ctx.paths,
            &ctx.campaign,
            &location_id,
            &sorted,
            &excluded_real_ids,
        )
        .await?;

        if snapshot.real_claimable.is_empty() && snapshot.consolation.is_empty() {
            break;
        }

        let entry = SpinService::select_entry(&snapshot, &mut rand::thread_rng());
        if entry.1.is_empty() {
            break;
        }

        let is_real = is_real_prize(&entry.0);

        match SpinService::finalize_spin_claim(
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
        return SpinService::finalize_spin_claim(
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
