use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::infrastructure::CampaignPaths;
use crate::features::inventory::infrastructure::InventoryRepository;
use crate::models::prize::PrizeModel;
use crate::models::question::QuestionModel;
use crate::models::registration::RegistrationModel;
use crate::utils::firestore::millis_now;
use anyhow::anyhow;
use firestore::errors::{
    BackoffError, FirestoreError, FirestoreInvalidParametersError,
    FirestoreInvalidParametersPublicDetails,
};
use firestore::FirestoreQueryDirection;
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;

const SUBMISSIONS_SUBCOL: &str = "submissions";
const SESSIONS_SUBCOL: &str = "sessions";
const REGISTRATIONS_SUBCOL: &str = "registrations";

pub struct SubmissionModel;

#[derive(Debug)]
pub struct SubmissionCreateInput {
    pub session_id: String,
    pub full_name: String,
    pub normalized_name: String,
    pub answers: Vec<i64>,
    pub user_agent: String,
    pub ip: String,
    pub location_id: Option<String>,
    pub geo_status: String,
}

#[derive(Debug)]
pub struct FinalizeSpinResult {
    pub finalized: bool,
    pub previous_prize: Option<String>,
}

impl SubmissionModel {
    pub async fn find_by_id(
        state: &AppState,
        paths: &CampaignPaths,
        id: &str,
    ) -> ApiResult<Option<Map<String, Value>>> {
        let parent = paths.parent_str(&state.db.client)?;
        state
            .db
            .client
            .fluent()
            .select()
            .by_id_in(SUBMISSIONS_SUBCOL)
            .parent(parent)
            .obj()
            .one(id)
            .await
            .map_err(|e| ApiError::Internal(e.into()))
    }

    pub async fn find_page(
        state: &AppState,
        paths: &CampaignPaths,
        limit: usize,
        _cursor: Option<Map<String, Value>>,
    ) -> ApiResult<(Vec<Map<String, Value>>, Option<Map<String, Value>>, bool)> {
        let parent = paths.parent_str(&state.db.client)?;
        let cap = limit.clamp(1, 200);
        let items: Vec<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .from(SUBMISSIONS_SUBCOL)
            .parent(parent)
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

    pub async fn ids_that_exist(
        state: &AppState,
        paths: &CampaignPaths,
        ids: &[String],
    ) -> ApiResult<HashSet<String>> {
        let mut existing = HashSet::new();
        for id in ids {
            if Self::find_by_id(state, paths, id).await?.is_some() {
                existing.insert(id.clone());
            }
        }
        Ok(existing)
    }

    pub async fn find_for_raffle_pool(
        state: &AppState,
        paths: &CampaignPaths,
        min_score: i64,
        prize_winners_only: bool,
    ) -> ApiResult<Vec<Map<String, Value>>> {
        let parent = paths.parent_str(&state.db.client)?;
        let rows: Vec<Map<String, Value>> = if min_score > 0 {
            state
                .db
                .client
                .fluent()
                .select()
                .from(SUBMISSIONS_SUBCOL)
                .parent(parent)
                .filter(|q| q.field("score").greater_than_or_equal(min_score))
                .obj::<Map<String, Value>>()
                .query()
                .await
                .map_err(|e| ApiError::Internal(e.into()))?
        } else {
            state
                .db
                .client
                .fluent()
                .select()
                .from(SUBMISSIONS_SUBCOL)
                .parent(parent)
                .obj::<Map<String, Value>>()
                .query()
                .await
                .map_err(|e| ApiError::Internal(e.into()))?
        };

        Ok(if prize_winners_only {
            rows.into_iter()
                .filter(|s| {
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

    pub async fn create(
        state: &AppState,
        paths: &CampaignPaths,
        input: SubmissionCreateInput,
    ) -> ApiResult<Map<String, Value>> {
        let questions = QuestionModel::find_all(state, paths).await?;
        if questions.is_empty() {
            return Err(ApiError::WithStatus {
                status: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                message: "No questions configured.".into(),
                code: Some("NO_QUESTIONS".into()),
                data: None,
            });
        }

        if input.geo_status == "outside" {
            return Err(ApiError::with_code(
                axum::http::StatusCode::FORBIDDEN,
                "GEO_OUTSIDE_ZONE",
                "Participation is restricted to allowed geographic zones.",
            ));
        }

        if input.answers.len() != questions.len() {
            return Err(ApiError::WithStatus {
                status: axum::http::StatusCode::BAD_REQUEST,
                message: "answers must match the number of questions.".into(),
                code: Some("INVALID_ANSWERS_LENGTH".into()),
                data: None,
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
                    data: None,
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

        let parent = paths.parent_str(&state.db.client)?;
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
            "locationId": input.location_id,
            "geoStatus": input.geo_status,
            "status": status,
            "submittedAt": millis_now(),
        });

        let session_id_for_tx = session_id.clone();
        let payload_for_tx = payload.clone();
        let prize_owned = prize.to_string();
        let status_owned = status.to_string();
        let parent_for_tx = parent.clone();
        let result = db
            .run_transaction(move |db, transaction| {
                create_tx(
                    db,
                    transaction,
                    parent_for_tx.clone(),
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

    pub async fn delete(state: &AppState, paths: &CampaignPaths, id: &str) -> ApiResult<()> {
        let sub = Self::find_by_id(state, paths, id).await?;
        let reg = RegistrationModel::find_by_id(state, paths, id).await?;

        let mut decrement: Option<(String, String)> = None;
        if let Some(sub) = &sub {
            if let (Some(prize_name), Some(location_id)) = (
                sub.get("prize").and_then(|v| v.as_str()),
                sub.get("locationId").and_then(|v| v.as_str()),
            ) {
                if prize_name != "pending" && prize_name != "Nothing" {
                    let prizes = PrizeModel::find_all(state, paths).await?;
                    if let Some(prize) = prizes.iter().find(|p| {
                        p.get("name").and_then(|n| n.as_str()) == Some(prize_name)
                            && p.get("isRealPrize").and_then(|v| v.as_bool()).unwrap_or(true)
                    }) {
                        if let Some(prize_id) = prize.get("id").and_then(|v| v.as_str()) {
                            decrement = Some((location_id.to_string(), prize_id.to_string()));
                        }
                    }
                }
            }
        }

        let parent = paths.parent_str(&state.db.client)?;
        let writer = state.db.batch_writer().await.map_err(|e| ApiError::Internal(e.into()))?;
        let mut batch = writer.new_batch();

        delete_in_batch(&state.db.client, &mut batch, parent.as_str(), SUBMISSIONS_SUBCOL, id)?;
        delete_in_batch(&state.db.client, &mut batch, parent.as_str(), SESSIONS_SUBCOL, id)?;

        if reg.is_some() {
            delete_in_batch(
                &state.db.client,
                &mut batch,
                parent.as_str(),
                REGISTRATIONS_SUBCOL,
                id,
            )?;
            if let Some(reg) = reg {
                if let Some(normalized) = reg.get("normalizedName").and_then(|v| v.as_str()) {
                    delete_in_batch(
                        &state.db.client,
                        &mut batch,
                        parent.as_str(),
                        REGISTRATIONS_SUBCOL,
                        &format!("name_{normalized}"),
                    )?;
                }
            }
        }

        batch.write().await.map_err(|e| ApiError::Internal(e.into()))?;

        if let Some((location_id, prize_id)) = decrement {
            InventoryRepository::decrement_on_delete(state, paths, &location_id, &prize_id).await?;
        }

        Ok(())
    }
}

fn create_tx<'b>(
    db: firestore::FirestoreDb,
    transaction: &'b mut firestore::FirestoreTransaction,
    parent: String,
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
            .by_id_in(SESSIONS_SUBCOL)
            .parent(parent.as_str())
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
            .by_id_in(SUBMISSIONS_SUBCOL)
            .parent(parent.as_str())
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
            .in_col(SUBMISSIONS_SUBCOL)
            .document_id(&session_id)
            .parent(parent.as_str())
            .object(&payload)
            .add_to_transaction(transaction)
            .map_err(BackoffError::Permanent)?;

        db.fluent()
            .update()
            .in_col(SESSIONS_SUBCOL)
            .document_id(&session_id)
            .parent(parent.as_str())
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

fn delete_in_batch(
    db: &firestore::FirestoreDb,
    batch: &mut firestore::FirestoreBatch<'_, firestore::FirestoreSimpleBatchWriter>,
    parent: &str,
    collection: &str,
    id: &str,
) -> ApiResult<()> {
    db.fluent()
        .delete()
        .from(collection)
        .parent(parent)
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
            data: None,
        };
    }
    ApiError::Internal(anyhow!(err))
}
