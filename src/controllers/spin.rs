use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::logger;
use crate::middleware::request_context::RequestContext;
use crate::models::prize::PrizeModel;
use crate::models::spin_token::SpinTokenModel;
use crate::models::submission::SubmissionModel;
use crate::utils::challenge_window::assert_challenge_open;
use crate::utils::spin_token::verify_spin_token;
use axum::{extract::State, Extension, Json};
use rand::Rng;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;

const DEFAULT_ANIMATION_MS: i64 = 5000;

#[derive(Deserialize)]
pub struct SpinBody {
    #[serde(rename = "spinToken")]
    pub spin_token: Option<String>,
}

pub async fn spin_wheel(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<RequestContext>,
    Json(body): Json<SpinBody>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    assert_challenge_open(&state).await?;

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

    let session_id = verify_spin_token(&state.config, spin_token)?;

    let fingerprint = hex::encode(Sha256::digest(spin_token.as_bytes()));

    let existing = SubmissionModel::find_by_id(&state, &session_id)
        .await?
        .ok_or_else(|| ApiError::bad_request("Submission not found for this session."))?;

    if existing.get("prize").and_then(|v| v.as_str()) != Some("pending") {
        let prize_name = existing.get("prize").and_then(|v| v.as_str()).unwrap_or("");
        let _ = SpinTokenModel::mark_consumed(&state, &fingerprint, &session_id).await;
        let prizes = PrizeModel::find_all(&state).await?;
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

    if !SpinTokenModel::consume_if_fresh(&state, &fingerprint, &session_id).await? {
        logger::log(
            &state.config,
            "warn",
            "spin_token_replay_attempt",
            json!({
                "requestId": ctx.request_id,
                "sessionIdPrefix": &session_id[..session_id.len().min(10)],
            }),
        );
        return Err(ApiError::with_code(
            axum::http::StatusCode::CONFLICT,
            "SPIN_TOKEN_ALREADY_USED",
            "This spin token has already been used.",
        ));
    }

    let prizes = PrizeModel::find_all(&state).await?;
    let prize_counts = SubmissionModel::get_prize_counts(&state).await?;

    if prizes.is_empty() {
        logger::log(
            &state.config,
            "error",
            "spin_no_prizes",
            json!({ "requestId": ctx.request_id }),
        );
        return Err(ApiError::bad_request("No prizes configured."));
    }

    let mut sorted = prizes.clone();
    sorted.sort_by_key(|p| p.get("order").and_then(|v| v.as_i64()).unwrap_or(0));
    let sorted_for_idempotent = sorted.clone();

    let available: Vec<_> = sorted
        .iter()
        .filter(|p| {
            let is_real = p.get("isRealPrize").and_then(|v| v.as_bool()).unwrap_or(true);
            if !is_real {
                return true;
            }
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let count = prize_counts.get(name).copied().unwrap_or(0);
            count < state.config.real_prize_limit as i64
        })
        .cloned()
        .collect();

    let pool: Vec<_> = if !available.is_empty() {
        available
    } else {
        sorted
            .into_iter()
            .filter(|p| !p.get("isRealPrize").and_then(|v| v.as_bool()).unwrap_or(true))
            .collect()
    };

    if pool.is_empty() {
        return Err(ApiError::bad_request("Prize inventory exhausted."));
    }

    let pick_idx = rand::thread_rng().gen_range(0..pool.len());
    let won = &pool[pick_idx];
    let prize_name = won.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let order = won.get("order").and_then(|v| v.as_i64()).unwrap_or(0);
    let is_real = won.get("isRealPrize").and_then(|v| v.as_bool()).unwrap_or(false);

    let result = SubmissionModel::finalize_spin_prize(
        &state,
        &session_id,
        prize_name,
        is_real,
    )
    .await?;

    logger::log(
        &state.config,
        "info",
        "spin_audit",
        json!({
            "requestId": ctx.request_id,
            "sessionIdPrefix": &session_id[..session_id.len().min(10)],
            "prize": prize_name,
            "isRealPrize": is_real,
            "finalized": result.finalized,
            "tokenFingerprint": &fingerprint[..fingerprint.len().min(16)],
        }),
    );

    if !result.finalized {
        if let Some(prev_name) = result.previous_prize {
            let prev = sorted_for_idempotent
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

    Ok(SuccessResponse::data(json!({
        "prize": {
            "name": prize_name,
            "order": order,
            "isRealPrize": is_real,
        },
        "animationDuration": DEFAULT_ANIMATION_MS,
    })))
}
