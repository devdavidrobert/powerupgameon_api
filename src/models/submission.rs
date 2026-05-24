use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::infrastructure::CampaignPaths;
use crate::features::inventory::domain::InventorySlot;
use crate::features::locations::domain::GeoStatus;
use crate::models::prize::PrizeModel;
use crate::models::question::QuestionModel;
use crate::models::registration::RegistrationModel;
use crate::utils::firestore::{
    build_page_cursor, document_id_from_map, millis_from_value, millis_now, start_after_cursor,
};
use crate::utils::firestore_tx::tx_get_optional;
use anyhow::anyhow;
use firestore::errors::{
    BackoffError, FirestoreError, FirestoreInvalidParametersError,
    FirestoreInvalidParametersPublicDetails,
};
use firestore::FirestoreQueryDirection;
use futures_util::StreamExt;
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;

const INVENTORY_SUBCOL: &str = "inventory";
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
    pub geo_status: GeoStatus,
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
        cursor: Option<Map<String, Value>>,
    ) -> ApiResult<(Vec<Map<String, Value>>, Option<Map<String, Value>>, bool)> {
        let (mut items, next_cursor, has_more) =
            Self::find_page_ordered_by_submitted_at(state, paths, limit, cursor.clone()).await?;

        // Legacy rows: spin finalization used to replace the whole doc and drop `submittedAt`,
        // so they are invisible to the composite index query above.
        if items.is_empty() && cursor.is_none() {
            let fallback = Self::find_page_by_document_name(state, paths, limit, None).await?;
            items = fallback.0;
            return Ok((items, fallback.1, fallback.2));
        }

        Ok((items, next_cursor, has_more))
    }

    async fn find_page_ordered_by_submitted_at(
        state: &AppState,
        paths: &CampaignPaths,
        limit: usize,
        cursor: Option<Map<String, Value>>,
    ) -> ApiResult<(Vec<Map<String, Value>>, Option<Map<String, Value>>, bool)> {
        let parent = paths.parent_str(&state.db.client)?;
        let cap = limit.clamp(1, 200);
        let query_limit = (cap + 1) as u32;

        let mut query = state
            .db
            .client
            .fluent()
            .select()
            .from(SUBMISSIONS_SUBCOL)
            .parent(parent.as_str())
            .order_by([
                ("submittedAt", FirestoreQueryDirection::Descending),
                ("__name__", FirestoreQueryDirection::Descending),
            ]);

        if let Some(ref cursor_map) = cursor {
            query = query.start_at(start_after_cursor(cursor_map, "submittedAt")?);
        }

        let mut items: Vec<Map<String, Value>> = query
            .limit(query_limit)
            .obj()
            .query()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        let has_more = items.len() > cap;
        if has_more {
            items.truncate(cap);
        }
        let next_cursor = if has_more {
            items.last().and_then(|row| {
                build_page_cursor(row, "submittedAt", parent.as_str(), SUBMISSIONS_SUBCOL)
            })
        } else {
            None
        };
        Ok((items, next_cursor, has_more))
    }

    /// Lists every submission document (including legacy rows missing `submittedAt`).
    async fn find_page_by_document_name(
        state: &AppState,
        paths: &CampaignPaths,
        limit: usize,
        cursor: Option<Map<String, Value>>,
    ) -> ApiResult<(Vec<Map<String, Value>>, Option<Map<String, Value>>, bool)> {
        let parent = paths.parent_str(&state.db.client)?;
        let cap = limit.clamp(1, 200);
        let query_limit = (cap + 1) as u32;

        let mut query = state
            .db
            .client
            .fluent()
            .select()
            .from(SUBMISSIONS_SUBCOL)
            .parent(parent.as_str())
            .order_by([("__name__", FirestoreQueryDirection::Descending)]);

        if let Some(ref cursor_map) = cursor {
            query = query.start_at(start_after_cursor(cursor_map, "submittedAt")?);
        }

        let mut items: Vec<Map<String, Value>> = query
            .limit(query_limit)
            .obj()
            .query()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        items.sort_by(|a, b| list_sort_timestamp(b).cmp(&list_sort_timestamp(a)));

        let has_more = items.len() > cap;
        if has_more {
            items.truncate(cap);
        }
        let next_cursor = if has_more {
            items.last().and_then(|row| list_page_cursor(row, parent.as_str()))
        } else {
            None
        };
        Ok((items, next_cursor, has_more))
    }

    /// Backfill `submittedAt` on legacy spin-wiped rows so they appear in the indexed list query.
    pub async fn repair_list_index_fields(
        state: &AppState,
        paths: &CampaignPaths,
        id: &str,
        row: &Map<String, Value>,
    ) -> ApiResult<()> {
        if row.get("submittedAt").and_then(millis_from_value).is_some() {
            return Ok(());
        }
        let Some(ts) = row
            .get("finalizedAt")
            .and_then(millis_from_value)
            .or_else(|| row.get("_firestore_created").and_then(|v| v.as_str()).and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(s)
                    .ok()
                    .map(|dt| dt.timestamp_millis())
            }))
        else {
            return Ok(());
        };

        let parent = paths.parent_str(&state.db.client)?;
        let mut patch: Map<String, Value> = row
            .iter()
            .filter(|(k, _)| !k.starts_with("_firestore"))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        patch.insert("submittedAt".into(), json!(ts));
        state
            .db
            .client
            .fluent()
            .update()
            .in_col(SUBMISSIONS_SUBCOL)
            .document_id(id)
            .parent(parent.as_str())
            .object(&patch)
            .execute::<Map<String, Value>>()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
        Ok(())
    }

    pub async fn ids_that_exist(
        state: &AppState,
        paths: &CampaignPaths,
        ids: &[String],
    ) -> ApiResult<HashSet<String>> {
        if ids.is_empty() {
            return Ok(HashSet::new());
        }

        let parent = paths.parent_str(&state.db.client)?;
        let mut existing = HashSet::new();

        for chunk in ids.chunks(100) {
            let mut stream = state
                .db
                .client
                .fluent()
                .select()
                .by_id_in(SUBMISSIONS_SUBCOL)
                .parent(parent.as_str())
                .obj::<Map<String, Value>>()
                .batch(chunk)
                .await
                .map_err(|e| ApiError::Internal(e.into()))?;

            while let Some((id, doc)) = stream.next().await {
                if doc.is_some() {
                    existing.insert(id);
                }
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

        // Legacy defense-in-depth for registrations created before GPS-outside blocking.
        if input.geo_status == GeoStatus::Outside {
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
        let status = if prize == "pending" {
            "pending"
        } else {
            "completed"
        };

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
            "geoStatus": input.geo_status.as_str(),
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
        let prizes = PrizeModel::find_all(state, paths).await?;
        let sub = Self::find_by_id(state, paths, id).await?;
        let reg = RegistrationModel::find_by_id(state, paths, id).await?;

        let decrement = sub
            .as_ref()
            .and_then(|sub| resolve_inventory_decrement(sub, &prizes));
        let delete_registration = reg.is_some();
        let normalized_name = reg
            .as_ref()
            .and_then(|r| r.get("normalizedName").and_then(|v| v.as_str()))
            .map(String::from);

        let db = state.db.client.clone();
        let parent = paths.parent_str(&state.db.client)?;
        let id = id.to_string();

        db.run_transaction(move |db, transaction| {
            delete_submission_tx(
                db,
                transaction,
                parent.clone(),
                id.clone(),
                delete_registration,
                normalized_name.clone(),
                decrement.clone(),
            )
        })
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(())
    }
}

fn resolve_inventory_decrement(
    sub: &Map<String, Value>,
    prizes: &[Map<String, Value>],
) -> Option<(String, String)> {
    let prize_name = sub.get("prize").and_then(|v| v.as_str())?;
    let location_id = sub.get("locationId").and_then(|v| v.as_str())?;
    if prize_name == "pending" || prize_name == "Nothing" {
        return None;
    }
    let prize = prizes.iter().find(|p| {
        p.get("name").and_then(|n| n.as_str()) == Some(prize_name)
            && p.get("isRealPrize")
                .and_then(|v| v.as_bool())
                .unwrap_or(true)
    })?;
    let prize_id = prize.get("id").and_then(|v| v.as_str())?;
    Some((location_id.to_string(), prize_id.to_string()))
}

fn delete_submission_tx<'b>(
    db: firestore::FirestoreDb,
    transaction: &'b mut firestore::FirestoreTransaction,
    parent: String,
    id: String,
    delete_registration: bool,
    normalized_name: Option<String>,
    decrement: Option<(String, String)>,
) -> Pin<Box<dyn Future<Output = Result<(), BackoffError<FirestoreError>>> + Send + 'b>> {
    Box::pin(async move {
        if let Some((location_id, prize_id)) = &decrement {
            let slot_id = InventorySlot::slot_key(location_id, prize_id);
            let slot_doc =
                tx_get_optional(&db, parent.as_str(), INVENTORY_SUBCOL, &slot_id).await?;

            if let Some(slot_doc) = slot_doc {
                let awarded = slot_doc
                    .get("awardedCount")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                if awarded > 0 {
                    db.fluent()
                        .update()
                        .in_col(INVENTORY_SUBCOL)
                        .document_id(&slot_id)
                        .parent(parent.as_str())
                        .object(&json!({
                            "awardedCount": awarded - 1,
                            "updatedAt": millis_now(),
                        }))
                        .add_to_transaction(transaction)
                        .map_err(BackoffError::Permanent)?;
                }
            }
        }

        delete_doc_tx(&db, transaction, &parent, SUBMISSIONS_SUBCOL, &id)?;
        delete_doc_tx(&db, transaction, &parent, SESSIONS_SUBCOL, &id)?;

        if delete_registration {
            delete_doc_tx(&db, transaction, &parent, REGISTRATIONS_SUBCOL, &id)?;
            if let Some(normalized) = normalized_name {
                delete_doc_tx(
                    &db,
                    transaction,
                    &parent,
                    REGISTRATIONS_SUBCOL,
                    &format!("name_{normalized}"),
                )?;
            }
        }

        Ok(())
    })
}

fn delete_doc_tx(
    db: &firestore::FirestoreDb,
    transaction: &mut firestore::FirestoreTransaction,
    parent: &str,
    collection: &str,
    id: &str,
) -> Result<(), BackoffError<FirestoreError>> {
    db.fluent()
        .delete()
        .from(collection)
        .parent(parent)
        .document_id(id)
        .add_to_transaction(transaction)
        .map_err(BackoffError::Permanent)?;
    Ok(())
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
) -> Pin<
    Box<dyn Future<Output = Result<Map<String, Value>, BackoffError<FirestoreError>>> + Send + 'b>,
> {
    let prize = prize.to_string();
    let status = status.to_string();
    Box::pin(async move {
        let session_doc =
            tx_get_optional(&db, parent.as_str(), SESSIONS_SUBCOL, &session_id).await?;
        if session_doc.is_none() {
            return Err(BackoffError::Permanent(
                FirestoreError::InvalidParametersError(FirestoreInvalidParametersError::new(
                    FirestoreInvalidParametersPublicDetails::new(
                        "NO_SESSION".to_string(),
                        "code".to_string(),
                    ),
                )),
            ));
        }

        let sub_doc =
            tx_get_optional(&db, parent.as_str(), SUBMISSIONS_SUBCOL, &session_id).await?;
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

fn list_sort_timestamp(row: &Map<String, Value>) -> i64 {
    row.get("submittedAt")
        .and_then(millis_from_value)
        .or_else(|| row.get("finalizedAt").and_then(millis_from_value))
        .unwrap_or(0)
}

fn list_page_cursor(row: &Map<String, Value>, parent: &str) -> Option<Map<String, Value>> {
    let ts = list_sort_timestamp(row);
    let id = document_id_from_map(row)?;
    let name = crate::utils::firestore::document_ref_path(parent, SUBMISSIONS_SUBCOL, &id);
    let mut cursor = Map::new();
    cursor.insert("submittedAt".into(), json!(ts));
    cursor.insert("name".into(), json!(name));
    Some(cursor)
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
