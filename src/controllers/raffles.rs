use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::models::raffle::RaffleModel;
use crate::models::submission::SubmissionModel;
use crate::utils::firestore::value_to_iso;
use crate::utils::helpers::fisher_yates_shuffle;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Local;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::sync::Arc;

pub async fn get_all_raffles(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<SuccessResponse<Vec<Map<String, Value>>>>> {
    let raffles = RaffleModel::find_all_raffles(&state).await?;
    let data = raffles
        .into_iter()
        .map(|mut r| {
            if let Some(created) = r.get("createdAt") {
                if let Some(iso) = value_to_iso(created) {
                    r.insert("createdAt".into(), json!(iso));
                }
            }
            r
        })
        .collect();
    Ok(SuccessResponse::data(data))
}

pub async fn get_raffle_winners(
    State(state): State<Arc<AppState>>,
    Path(raffle_id): Path<String>,
) -> ApiResult<Json<SuccessResponse<Vec<Map<String, Value>>>>> {
    if RaffleModel::find_raffle_by_id(&state, &raffle_id)
        .await?
        .is_none()
    {
        return Err(ApiError::bad_request("Raffle not found."));
    }

    let winners = RaffleModel::find_winners_by_raffle(&state, &raffle_id).await?;
    let data = winners
        .into_iter()
        .map(|mut w| {
            if let Some(selected) = w.get("selectedAt") {
                if let Some(iso) = value_to_iso(selected) {
                    w.insert("selectedAt".into(), json!(iso));
                }
            }
            w
        })
        .collect();
    Ok(SuccessResponse::data(data))
}

#[derive(Deserialize)]
pub struct CreateRaffleBody {
    #[serde(rename = "winnerCount")]
    pub winner_count: Option<i64>,
    #[serde(rename = "minScore")]
    pub min_score: Option<i64>,
    #[serde(rename = "prizeWinnersOnly")]
    pub prize_winners_only: Option<bool>,
}

pub async fn create_raffle(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateRaffleBody>,
) -> ApiResult<(StatusCode, Json<SuccessResponse<Value>>)> {
    let winner_count = body.winner_count.unwrap_or(0);
    if winner_count < 1 {
        return Err(ApiError::bad_request("winnerCount must be at least 1."));
    }

    let pool = SubmissionModel::find_for_raffle_pool(
        &state,
        body.min_score.unwrap_or(0),
        body.prize_winners_only.unwrap_or(false),
    )
    .await?;

    if pool.is_empty() {
        return Err(ApiError::WithStatus {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: "No submissions match the selected criteria.".into(),
            code: None,
        });
    }

    if winner_count as usize > pool.len() {
        return Err(ApiError::WithStatus {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: format!(
                "Only {} players match the criteria. Cannot pick {} winners.",
                pool.len(),
                winner_count
            ),
            code: None,
        });
    }

    let shuffled = fisher_yates_shuffle(pool);
    let selected = shuffled.into_iter().take(winner_count as usize);

    let now = Local::now();
    let raffle_name = format!(
        "Raffle Draw - {} {}",
        now.format("%m/%d/%Y"),
        now.format("%I:%M %p")
    );

    let winners: Vec<Map<String, Value>> = selected
        .map(|s| {
            let mut m = Map::new();
            m.insert(
                "originalSubmissionId".into(),
                s.get("id")
                    .or_else(|| s.get("sessionId"))
                    .cloned()
                    .unwrap_or(Value::Null),
            );
            m.insert(
                "fullName".into(),
                s.get("fullName").cloned().unwrap_or(Value::Null),
            );
            m.insert("prize".into(), s.get("prize").cloned().unwrap_or(Value::Null));
            m.insert("score".into(), s.get("score").cloned().unwrap_or(Value::Null));
            m.insert(
                "percentage".into(),
                s.get("percentage").cloned().unwrap_or(Value::Null),
            );
            m.insert(
                "submittedAt".into(),
                s.get("submittedAt").cloned().unwrap_or(Value::Null),
            );
            m
        })
        .collect();

    let (raffle, winner_rows) =
        RaffleModel::create_raffle_with_winners(&state, &raffle_name, winners).await?;

    let response = json!({
        "raffle": serialize_raffle(&raffle),
        "winners": winner_rows.iter().map(serialize_winner).collect::<Vec<_>>(),
    });

    Ok((StatusCode::CREATED, SuccessResponse::data(response)))
}

#[derive(Deserialize)]
pub struct UpdateWinnerBody {
    #[serde(rename = "giftReceived")]
    pub gift_received: Option<bool>,
}

pub async fn update_winner_gift_status(
    State(state): State<Arc<AppState>>,
    Path(winner_id): Path<String>,
    Json(body): Json<UpdateWinnerBody>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    let Some(gift_received) = body.gift_received else {
        return Err(ApiError::bad_request("giftReceived must be a boolean."));
    };
    RaffleModel::update_gift_received(&state, &winner_id, gift_received).await?;
    Ok(SuccessResponse::message("Gift status updated."))
}

fn serialize_raffle(raffle: &Map<String, Value>) -> Value {
    let mut out = raffle.clone();
    if let Some(created) = out.get("createdAt") {
        if let Some(iso) = value_to_iso(created) {
            out.insert("createdAt".into(), json!(iso));
        }
    }
    json!(out)
}

fn serialize_winner(winner: &Map<String, Value>) -> Value {
    let mut out = winner.clone();
    if let Some(selected) = out.get("selectedAt") {
        if let Some(iso) = value_to_iso(selected) {
            out.insert("selectedAt".into(), json!(iso));
        }
    }
    json!(out)
}
