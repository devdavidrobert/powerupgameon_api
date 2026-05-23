use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::models::question::QuestionModel;
use crate::models::registration::RegistrationModel;
use crate::utils::firestore::millis_now;
use anyhow::anyhow;
use firestore::errors::{
    BackoffError, FirestoreDataNotFoundError, FirestoreError, FirestoreErrorPublicGenericDetails,
    FirestoreInvalidParametersError, FirestoreInvalidParametersPublicDetails,
};
use firestore::FirestoreQueryDirection;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;

const COLLECTION: &str = "submissions";
const SESSIONS_COLLECTION: &str = "sessions";
const AGGREGATES_COLLECTION: &str = "system";
const AGGREGATES_DOC: &str = "aggregates";

pub struct SubmissionModel;

#[derive(Debug)]
pub struct SubmissionCreateInput {
    pub session_id: String,
    pub full_name: String,
    pub normalized_name: String,
    pub answers: Vec<i64>,
    pub user_agent: String,
    pub ip: String,
}

#[derive(Debug)]
pub struct FinalizeSpinResult {
    pub finalized: bool,
    pub previous_prize: Option<String>,
}

impl SubmissionModel {
    pub async fn find_by_id(state: &AppState, id: &str) -> ApiResult<Option<Map<String, Value>>> {
        state
            .db
            .client
            .fluent()
            .select()
            .by_id_in(COLLECTION)
            .obj()
            .one(id)
            .await
            .map_err(|e| ApiError::Internal(e.into()))
    }

    pub async fn find_page(
        state: &AppState,
        limit: usize,
        _cursor: Option<Map<String, Value>>,
    ) -> ApiResult<(Vec<Map<String, Value>>, Option<Map<String, Value>>, bool)> {
        let cap = limit.clamp(1, 200);
        let items: Vec<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .from(COLLECTION)
            .order_by([
                ("submittedAt", FirestoreQueryDirection::Descending),
                ("__name__", FirestoreQueryDirection::Descending),
            ])
            .limit(cap as u32)
            .obj()
            .query()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        let has_more = items.len() == cap;
        Ok((items, None, has_more))
    }

    pub async fn ids_that_exist(state: &AppState, ids: &[String]) -> ApiResult<HashSet<String>> {
        let mut existing = HashSet::new();
        for id in ids {
            if Self::find_by_id(state, id).await?.is_some() {
                existing.insert(id.clone());
            }
        }
        Ok(existing)
    }

    pub async fn find_for_raffle_pool(
        state: &AppState,
        min_score: i64,
        prize_winners_only: bool,
    ) -> ApiResult<Vec<Map<String, Value>>> {
        let rows: Vec<Map<String, Value>> = if min_score > 0 {
            state
                .db
                .client
                .fluent()
                .select()
                .from(COLLECTION)
                .filter(|q| q.field("score").greater_than_or_equal(min_score))
                .obj::<Map<String, Value>>()
                .query()
                .await
                .map_err(|e| ApiError::Internal(e.into()))?
        } else if prize_winners_only {
            state
                .db
                .client
                .fluent()
                .select()
                .from(COLLECTION)
                .obj::<Map<String, Value>>()
                .query()
                .await
                .map_err(|e| ApiError::Internal(e.into()))?
                .into_iter()
                .filter(|s: &Map<String, Value>| {
                    matches!(
                        s.get("prize").and_then(|v| v.as_str()),
                        Some(p) if p != "Nothing" && p != "pending"
                    )
                })
                .collect()
        } else {
            state
                .db
                .client
                .fluent()
                .select()
                .from(COLLECTION)
                .obj()
                .query()
                .await
                .map_err(|e| ApiError::Internal(e.into()))?
        };

        Ok(if prize_winners_only && min_score > 0 {
            rows.into_iter()
                .filter(|s: &Map<String, Value>| {
                    matches!(
                        s.get("prize").and_then(|v| v.as_str()),
                        Some(p) if p != "Nothing" && p != "pending"
                    )
                })
                .collect()
        } else {
            rows
        })
    }

    pub async fn get_prize_counts(state: &AppState) -> ApiResult<HashMap<String, i64>> {
        let doc: Option<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .by_id_in(AGGREGATES_COLLECTION)
            .obj()
            .one(AGGREGATES_DOC)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        if let Some(doc) = doc {
            if let Some(counts) = doc.get("prizeAwardCounts").and_then(|v| v.as_object()) {
                if !counts.is_empty() {
                    return Ok(counts
                        .iter()
                        .filter_map(|(k, v)| v.as_i64().map(|n| (k.clone(), n)))
                        .collect());
                }
            }
        }

        Self::rebuild_prize_award_counts(state).await
    }

    pub async fn rebuild_prize_award_counts(state: &AppState) -> ApiResult<HashMap<String, i64>> {
        let snap: Vec<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .from(COLLECTION)
            .obj()
            .query()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        let mut counts: HashMap<String, i64> = HashMap::new();
        for doc in snap {
            if let Some(prize) = doc.get("prize").and_then(|v| v.as_str()) {
                if prize != "pending" && prize != "Nothing" {
                    *counts.entry(prize.to_string()).or_insert(0) += 1;
                }
            }
        }

        state
            .db
            .client
            .fluent()
            .update()
            .in_col(AGGREGATES_COLLECTION)
            .document_id(AGGREGATES_DOC)
            .object(&json!({
                "prizeAwardCounts": counts,
                "rebuiltAt": millis_now(),
            }))
            .execute::<Map<String, Value>>()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(counts)
    }

    pub async fn create(state: &AppState, input: SubmissionCreateInput) -> ApiResult<Map<String, Value>> {
        let questions = QuestionModel::find_all(state).await?;
        if questions.is_empty() {
            return Err(ApiError::WithStatus {
                status: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                message: "No questions configured.".into(),
                code: Some("NO_QUESTIONS".into()),
            });
        }

        if input.answers.len() != questions.len() {
            return Err(ApiError::WithStatus {
                status: axum::http::StatusCode::BAD_REQUEST,
                message: "answers must match the number of questions.".into(),
                code: Some("INVALID_ANSWERS_LENGTH".into()),
            });
        }

        let mut score = 0i64;
        for (i, q) in questions.iter().enumerate() {
            let ans = input.answers[i];
            let options_len = q
                .get("options")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0) as i64;
            let correct = q.get("correctIndex").and_then(|v| v.as_i64()).unwrap_or(-1);
            if ans < 0 || ans >= options_len {
                return Err(ApiError::WithStatus {
                    status: axum::http::StatusCode::BAD_REQUEST,
                    message: format!("Invalid answer index for question {i}."),
                    code: Some("INVALID_ANSWER_INDEX".into()),
                });
            }
            if ans == correct {
                score += 1;
            }
        }

        let total = questions.len() as i64;
        let percentage = ((score as f64 / total as f64) * 100.0).round() as i64;
        let prize = if score == total { "pending" } else { "Nothing" };
        let status = if prize == "pending" { "pending" } else { "completed" };

        let db = state.db.client.clone();
        let session_id = input.session_id.clone();
        let payload = json!({
            "sessionId": input.session_id,
            "fullName": input.full_name.to_uppercase(),
            "normalizedName": input.normalized_name,
            "score": score,
            "total": total,
            "percentage": percentage,
            "prize": prize,
            "answers": input.answers,
            "userAgent": input.user_agent,
            "ip": input.ip,
            "status": status,
            "submittedAt": millis_now(),
        });

        let session_id_for_tx = session_id.clone();
        let payload_for_tx = payload.clone();
        let prize_owned = prize.to_string();
        let status_owned = status.to_string();
        let result = db
            .run_transaction(move |db, transaction| {
                create_tx(
                    db,
                    transaction,
                    session_id_for_tx.clone(),
                    payload_for_tx.clone(),
                    score,
                    percentage,
                    &prize_owned,
                    &status_owned,
                )
            })
            .await
            .map_err(map_submission_error)?;

        Ok(result)
    }

    pub async fn finalize_spin_prize(
        state: &AppState,
        session_id: &str,
        prize_name: &str,
        is_real_prize: bool,
    ) -> ApiResult<FinalizeSpinResult> {
        let db = state.db.client.clone();
        let session_id = session_id.to_string();
        let prize_name = prize_name.to_string();

        db.run_transaction(move |db, transaction| {
            finalize_tx(
                db,
                transaction,
                session_id.clone(),
                prize_name.clone(),
                is_real_prize,
            )
        })
        .await
        .map_err(|e| ApiError::Internal(e.into()))
    }

    pub async fn delete(state: &AppState, id: &str) -> ApiResult<()> {
        let sub = Self::find_by_id(state, id).await?;
        let reg = RegistrationModel::find_by_id(state, id).await?;

        let mut decrement_prize = None;
        if let Some(sub) = &sub {
            if let Some(prize) = sub.get("prize").and_then(|v| v.as_str()) {
                if prize != "pending" && prize != "Nothing" {
                    let prizes = crate::models::prize::PrizeModel::find_all(state).await?;
                    if prizes.iter().any(|p| {
                        p.get("name").and_then(|n| n.as_str()) == Some(prize)
                            && p.get("isRealPrize").and_then(|v| v.as_bool()).unwrap_or(true)
                    }) {
                        decrement_prize = Some(prize.to_string());
                    }
                }
            }
        }

        let writer = state.db.batch_writer().await.map_err(|e| ApiError::Internal(e.into()))?;
        let mut batch = writer.new_batch();

        delete_in_batch(&state.db.client, &mut batch, COLLECTION, id)?;
        delete_in_batch(&state.db.client, &mut batch, SESSIONS_COLLECTION, id)?;

        if reg.is_some() {
            delete_in_batch(&state.db.client, &mut batch, "registrations", id)?;
            if let Some(reg) = reg {
                if let Some(normalized) = reg.get("normalizedName").and_then(|v| v.as_str()) {
                    delete_in_batch(
                        &state.db.client,
                        &mut batch,
                        "registrations",
                        &format!("name_{normalized}"),
                    )?;
                }
            }
        }

        if let Some(prize) = decrement_prize {
            let stats: Option<Map<String, Value>> = state
                .db
                .client
                .fluent()
                .select()
                .by_id_in(AGGREGATES_COLLECTION)
                .obj()
                .one(AGGREGATES_DOC)
                .await
                .map_err(|e| ApiError::Internal(e.into()))?;

            let mut prev = stats
                .as_ref()
                .and_then(|d| d.get("prizeAwardCounts"))
                .and_then(|v| v.as_object())
                .cloned()
                .unwrap_or_default();
            let current = prev.get(&prize).and_then(|v| v.as_i64()).unwrap_or(0);
            if current <= 1 {
                prev.remove(&prize);
            } else {
                prev.insert(prize.clone(), json!(current - 1));
            }

            state
                .db
                .client
                .fluent()
                .update()
                .in_col(AGGREGATES_COLLECTION)
                .document_id(AGGREGATES_DOC)
                .object(&json!({
                    "prizeAwardCounts": prev,
                    "updatedAt": millis_now(),
                }))
                .add_to_batch(&mut batch)
                .map_err(|e| ApiError::Internal(e.into()))?;
        }

        batch.write().await.map_err(|e| ApiError::Internal(e.into()))?;
        Ok(())
    }
}

fn create_tx<'b>(
    db: firestore::FirestoreDb,
    transaction: &'b mut firestore::FirestoreTransaction,
    session_id: String,
    payload: Value,
    score: i64,
    percentage: i64,
    prize: &str,
    status: &str,
) -> Pin<Box<dyn Future<Output = Result<Map<String, Value>, BackoffError<FirestoreError>>> + Send + 'b>> {
    let prize = prize.to_string();
    let status = status.to_string();
    Box::pin(async move {
        let session_doc: Option<Map<String, Value>> = db
            .fluent()
            .select()
            .by_id_in(SESSIONS_COLLECTION)
            .obj()
            .one(&session_id)
            .await
            .map_err(BackoffError::Permanent)?;
        if session_doc.is_none() {
            return Err(BackoffError::Permanent(FirestoreError::InvalidParametersError(
                FirestoreInvalidParametersError::new(FirestoreInvalidParametersPublicDetails::new(
                    "NO_SESSION".to_string(),
                    "code".to_string(),
                )),
            )));
        }

        let sub_doc: Option<Map<String, Value>> = db
            .fluent()
            .select()
            .by_id_in(COLLECTION)
            .obj()
            .one(&session_id)
            .await
            .map_err(BackoffError::Permanent)?;
        if let Some(existing) = sub_doc {
            let mut out = existing;
            out.insert("id".into(), json!(session_id));
            return Ok(out);
        }

        db.fluent()
            .update()
            .in_col(COLLECTION)
            .document_id(&session_id)
            .object(&payload)
            .add_to_transaction(transaction)
            .map_err(BackoffError::Permanent)?;

        db.fluent()
            .update()
            .in_col(SESSIONS_COLLECTION)
            .document_id(&session_id)
            .object(&json!({
                "hasPlayed": true,
                "playedAt": millis_now(),
                "score": score,
                "percentage": percentage,
                "prize": prize,
                "status": status,
            }))
            .add_to_transaction(transaction)
            .map_err(BackoffError::Permanent)?;

        let mut out = payload.as_object().cloned().unwrap_or_default();
        out.insert("id".into(), json!(session_id));
        Ok(out)
    })
}

fn finalize_tx<'b>(
    db: firestore::FirestoreDb,
    transaction: &'b mut firestore::FirestoreTransaction,
    session_id: String,
    prize_name: String,
    is_real_prize: bool,
) -> Pin<Box<dyn Future<Output = Result<FinalizeSpinResult, BackoffError<FirestoreError>>> + Send + 'b>> {
    Box::pin(async move {
        let sub_doc: Option<Map<String, Value>> = db
            .fluent()
            .select()
            .by_id_in(COLLECTION)
            .obj()
            .one(&session_id)
            .await
            .map_err(BackoffError::Permanent)?;
        let Some(sub) = sub_doc else {
            return Err(BackoffError::Permanent(FirestoreError::DataNotFoundError(
                FirestoreDataNotFoundError::new(
                    FirestoreErrorPublicGenericDetails::new("NOT_FOUND".to_string()),
                    "Submission not found".to_string(),
                ),
            )));
        };

        if let Some(existing_prize) = sub.get("prize").and_then(|v| v.as_str()) {
            if existing_prize != "pending" {
                return Ok(FinalizeSpinResult {
                    finalized: false,
                    previous_prize: Some(existing_prize.to_string()),
                });
            }
        }

        let now = millis_now();
        db.fluent()
            .update()
            .in_col(COLLECTION)
            .document_id(&session_id)
            .object(&json!({
                "prize": prize_name,
                "status": "completed",
                "finalizedAt": now,
            }))
            .add_to_transaction(transaction)
            .map_err(BackoffError::Permanent)?;

        db.fluent()
            .update()
            .in_col(SESSIONS_COLLECTION)
            .document_id(&session_id)
            .object(&json!({
                "prize": prize_name,
                "status": "completed",
            }))
            .add_to_transaction(transaction)
            .map_err(BackoffError::Permanent)?;

        if is_real_prize {
            let stats: Option<Map<String, Value>> = db
                .fluent()
                .select()
                .by_id_in(AGGREGATES_COLLECTION)
                .obj()
                .one(AGGREGATES_DOC)
                .await
                .map_err(BackoffError::Permanent)?;
            let mut prev = stats
                .as_ref()
                .and_then(|d| d.get("prizeAwardCounts"))
                .and_then(|v| v.as_object())
                .cloned()
                .unwrap_or_default();
            let current = prev.get(&prize_name).and_then(|v| v.as_i64()).unwrap_or(0);
            prev.insert(prize_name.clone(), json!(current + 1));
            db.fluent()
                .update()
                .in_col(AGGREGATES_COLLECTION)
                .document_id(AGGREGATES_DOC)
                .object(&json!({
                    "prizeAwardCounts": prev,
                    "updatedAt": now,
                }))
                .add_to_transaction(transaction)
                .map_err(BackoffError::Permanent)?;
        }

        Ok(FinalizeSpinResult {
            finalized: true,
            previous_prize: None,
        })
    })
}

fn delete_in_batch(
    db: &firestore::FirestoreDb,
    batch: &mut firestore::FirestoreBatch<'_, firestore::FirestoreSimpleBatchWriter>,
    collection: &str,
    id: &str,
) -> ApiResult<()> {
    db.fluent()
        .delete()
        .from(collection)
        .document_id(id)
        .add_to_batch(batch)
        .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(())
}

fn map_submission_error(err: FirestoreError) -> ApiError {
    let msg = err.to_string();
    if msg.contains("NO_SESSION") {
        return ApiError::WithStatus {
            status: axum::http::StatusCode::BAD_REQUEST,
            message: "No registration found for this session.".into(),
            code: Some("NO_SESSION".into()),
        };
    }
    ApiError::Internal(anyhow!(err))
}
