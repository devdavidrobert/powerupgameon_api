use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::domain::Campaign;
use crate::features::campaigns::infrastructure::CampaignPaths;
use crate::features::inventory::application::InventoryService;
use crate::features::inventory::domain::{InventorySlot, InventoryView};
use crate::features::locations::infrastructure::LocationRepository;
use crate::models::prize::PrizeModel;
use crate::models::submission::FinalizeSpinResult;
use crate::utils::firestore::{document_id_from_map, millis_now};
use crate::utils::firestore_tx::tx_get_optional;
use crate::utils::spin_token::spin_replay_expires_at_ms;
use firestore::errors::{
    BackoffError, FirestoreDataNotFoundError, FirestoreError, FirestoreErrorPublicGenericDetails,
    FirestoreInvalidParametersError, FirestoreInvalidParametersPublicDetails,
};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

const INVENTORY_SUBCOL: &str = "inventory";
const SUBMISSIONS_SUBCOL: &str = "submissions";
const SESSIONS_SUBCOL: &str = "sessions";
const SPIN_TOKENS_SUBCOL: &str = "spin_tokens";

pub struct InventoryRepository;

impl InventoryRepository {
    pub async fn find_all(
        state: &AppState,
        paths: &CampaignPaths,
    ) -> ApiResult<Vec<InventorySlot>> {
        let parent = paths.parent_str(&state.db.client)?;
        let rows: Vec<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .from(INVENTORY_SUBCOL)
            .parent(parent)
            .obj()
            .query()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(rows
            .into_iter()
            .filter_map(|mut row| {
                if !row.contains_key("id") {
                    if let Some(id) = document_id_from_map(&row) {
                        row.insert("id".into(), json!(id));
                    }
                }
                map_slot(row)
            })
            .collect())
    }

    pub async fn find_by_location(
        state: &AppState,
        paths: &CampaignPaths,
        location_id: &str,
    ) -> ApiResult<Vec<InventorySlot>> {
        let all = Self::find_all(state, paths).await?;
        Ok(all
            .into_iter()
            .filter(|s| s.location_id == location_id)
            .collect())
    }

    pub async fn find_slot(
        state: &AppState,
        paths: &CampaignPaths,
        location_id: &str,
        prize_id: &str,
    ) -> ApiResult<Option<InventorySlot>> {
        if location_id.trim().is_empty() || prize_id.trim().is_empty() {
            return Ok(None);
        }

        let id = InventorySlot::slot_key(location_id, prize_id);
        let parent = paths.parent_str(&state.db.client)?;
        let doc: Option<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .by_id_in(INVENTORY_SUBCOL)
            .parent(parent)
            .obj::<Map<String, Value>>()
            .one(&id)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(doc.and_then(|mut row| {
            row.entry("id").or_insert_with(|| json!(id));
            map_slot(row)
        }))
    }

    pub async fn upsert_slot(
        state: &AppState,
        paths: &CampaignPaths,
        location_id: &str,
        prize_id: &str,
        total_quantity: i64,
    ) -> ApiResult<InventorySlot> {
        if total_quantity < 0 {
            return Err(ApiError::bad_request("totalQuantity must be non-negative."));
        }

        let id = InventorySlot::slot_key(location_id, prize_id);
        let parent = paths.parent_str(&state.db.client)?;
        let existing = Self::find_slot(state, paths, location_id, prize_id).await?;
        let awarded = existing.as_ref().map(|s| s.awarded_count).unwrap_or(0);
        if total_quantity < awarded {
            return Err(ApiError::bad_request(
                "totalQuantity cannot be less than already awarded count.",
            ));
        }

        let now = millis_now();
        let payload = json!({
            "locationId": location_id,
            "prizeId": prize_id,
            "totalQuantity": total_quantity,
            "awardedCount": awarded,
            "updatedAt": now,
        });

        state
            .db
            .client
            .fluent()
            .update()
            .in_col(INVENTORY_SUBCOL)
            .document_id(&id)
            .parent(parent)
            .object(&payload)
            .execute::<Map<String, Value>>()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(InventorySlot {
            id,
            location_id: location_id.to_string(),
            prize_id: prize_id.to_string(),
            total_quantity,
            awarded_count: awarded,
            updated_at: Some(now),
        })
    }

    pub async fn build_views(
        state: &AppState,
        paths: &CampaignPaths,
        campaign: &Campaign,
    ) -> ApiResult<Vec<InventoryView>> {
        let slots = Self::find_all(state, paths).await?;
        let locations = LocationRepository::find_all(state, paths).await?;
        let prizes = PrizeModel::find_all(state, paths).await?;
        let now = millis_now();

        let loc_names: HashMap<String, String> = locations
            .iter()
            .map(|l| (l.id.clone(), l.name.clone()))
            .collect();
        let prize_names: HashMap<String, String> = prizes
            .iter()
            .filter_map(|p| {
                let id = crate::utils::firestore::document_id_from_map(p)?;
                let name = p.get("name")?.as_str()?;
                Some((id, name.to_string()))
            })
            .collect();

        Ok(slots
            .into_iter()
            .map(|slot| {
                let releasable = InventoryService::releasable_now(campaign, &slot, now);
                let remaining = slot.remaining(releasable);
                InventoryView {
                    location_id: slot.location_id.clone(),
                    location_name: loc_names
                        .get(&slot.location_id)
                        .cloned()
                        .unwrap_or_else(|| slot.location_id.clone()),
                    prize_id: slot.prize_id.clone(),
                    prize_name: prize_names
                        .get(&slot.prize_id)
                        .cloned()
                        .unwrap_or_else(|| slot.prize_id.clone()),
                    total_quantity: slot.total_quantity,
                    awarded_count: slot.awarded_count,
                    releasable_now: releasable,
                    remaining,
                }
            })
            .collect())
    }

    pub async fn claim_atomic(
        state: &AppState,
        paths: &CampaignPaths,
        campaign: &Campaign,
        session_id: &str,
        location_id: &str,
        prize_id: &str,
        prize_name: &str,
        is_real_prize: bool,
        token_fingerprint: &str,
    ) -> ApiResult<FinalizeSpinResult> {
        let db = state.db.client.clone();
        let parent = paths.parent_str(&state.db.client)?;
        let campaign_id = paths.campaign_id.clone();
        let session_id = session_id.to_string();
        let location_id = location_id.to_string();
        let prize_id = prize_id.to_string();
        let prize_name = prize_name.to_string();
        let token_fingerprint = token_fingerprint.to_string();
        let campaign = campaign.clone();
        let now = millis_now();
        let spin_replay_expires_at = spin_replay_expires_at_ms(&state.config, now);

        db.run_transaction(move |db, transaction| {
            claim_tx(
                db,
                transaction,
                parent.clone(),
                campaign_id.clone(),
                campaign.clone(),
                session_id.clone(),
                location_id.clone(),
                prize_id.clone(),
                prize_name.clone(),
                is_real_prize,
                token_fingerprint.clone(),
                now,
                spin_replay_expires_at,
            )
        })
        .await
        .map_err(map_claim_error)
    }

    pub async fn decrement_on_delete(
        state: &AppState,
        paths: &CampaignPaths,
        location_id: &str,
        prize_id: &str,
    ) -> ApiResult<()> {
        let slot = Self::find_slot(state, paths, location_id, prize_id).await?;
        let Some(slot) = slot else {
            return Ok(());
        };
        if slot.awarded_count <= 0 {
            return Ok(());
        }
        let parent = paths.parent_str(&state.db.client)?;
        let id = InventorySlot::slot_key(location_id, prize_id);
        state
            .db
            .client
            .fluent()
            .update()
            .in_col(INVENTORY_SUBCOL)
            .document_id(&id)
            .parent(parent)
            .object(&json!({
                "awardedCount": slot.awarded_count - 1,
                "updatedAt": millis_now(),
            }))
            .execute::<Map<String, Value>>()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
        Ok(())
    }
}

fn claim_tx<'b>(
    db: firestore::FirestoreDb,
    transaction: &'b mut firestore::FirestoreTransaction,
    parent: String,
    _campaign_id: String,
    campaign: Campaign,
    session_id: String,
    location_id: String,
    prize_id: String,
    prize_name: String,
    is_real_prize: bool,
    token_fingerprint: String,
    now: i64,
    spin_replay_expires_at: i64,
) -> Pin<
    Box<dyn Future<Output = Result<FinalizeSpinResult, BackoffError<FirestoreError>>> + Send + 'b>,
> {
    Box::pin(async move {
        let sub_doc =
            tx_get_optional(&db, parent.as_str(), SUBMISSIONS_SUBCOL, &session_id).await?;
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

        if is_real_prize {
            let slot_id = InventorySlot::slot_key(&location_id, &prize_id);
            let slot_doc =
                tx_get_optional(&db, parent.as_str(), INVENTORY_SUBCOL, &slot_id).await?;

            let slot = slot_doc.and_then(map_slot).ok_or_else(|| {
                BackoffError::Permanent(FirestoreError::InvalidParametersError(
                    FirestoreInvalidParametersError::new(
                        FirestoreInvalidParametersPublicDetails::new(
                            "INVENTORY_EXHAUSTED".to_string(),
                            "code".to_string(),
                        ),
                    ),
                ))
            })?;

            let releasable = InventoryService::releasable_now(&campaign, &slot, now);
            if !slot.is_claimable(releasable) {
                return Err(BackoffError::Permanent(
                    FirestoreError::InvalidParametersError(FirestoreInvalidParametersError::new(
                        FirestoreInvalidParametersPublicDetails::new(
                            "INVENTORY_EXHAUSTED".to_string(),
                            "code".to_string(),
                        ),
                    )),
                ));
            }

            db.fluent()
                .update()
                .in_col(INVENTORY_SUBCOL)
                .document_id(&slot_id)
                .parent(parent.as_str())
                .object(&json!({
                    "awardedCount": slot.awarded_count + 1,
                    "updatedAt": now,
                }))
                .add_to_transaction(transaction)
                .map_err(BackoffError::Permanent)?;
        }

        let token_doc =
            tx_get_optional(&db, parent.as_str(), SPIN_TOKENS_SUBCOL, &token_fingerprint).await?;
        if token_doc.is_some() {
            return Err(BackoffError::Permanent(
                FirestoreError::InvalidParametersError(FirestoreInvalidParametersError::new(
                    FirestoreInvalidParametersPublicDetails::new(
                        "SPIN_TOKEN_ALREADY_USED".to_string(),
                        "code".to_string(),
                    ),
                )),
            ));
        }

        db.fluent()
            .update()
            .in_col(SPIN_TOKENS_SUBCOL)
            .document_id(&token_fingerprint)
            .parent(parent.as_str())
            .object(&json!({
                "sessionId": session_id,
                "consumedAt": now,
                "expiresAt": spin_replay_expires_at,
            }))
            .add_to_transaction(transaction)
            .map_err(BackoffError::Permanent)?;

        let submission_update = merge_submission_spin_fields(&sub, &prize_name, now);
        db.fluent()
            .update()
            .in_col(SUBMISSIONS_SUBCOL)
            .document_id(&session_id)
            .parent(parent.as_str())
            .object(&submission_update)
            .add_to_transaction(transaction)
            .map_err(BackoffError::Permanent)?;

        db.fluent()
            .update()
            .in_col(SESSIONS_SUBCOL)
            .document_id(&session_id)
            .parent(parent.as_str())
            .object(&json!({
                "prize": prize_name,
                "status": "completed",
            }))
            .add_to_transaction(transaction)
            .map_err(BackoffError::Permanent)?;

        Ok(FinalizeSpinResult {
            finalized: true,
            previous_prize: None,
        })
    })
}

/// Firestore `.update().object()` replaces the whole document in our client — merge fields
/// so quiz scores, names, and `submittedAt` survive spin finalization (admin list depends on them).
fn merge_submission_spin_fields(
    existing: &Map<String, Value>,
    prize_name: &str,
    finalized_at: i64,
) -> Map<String, Value> {
    let mut merged: Map<String, Value> = existing
        .iter()
        .filter(|(k, _)| !k.starts_with("_firestore"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    merged.insert("prize".into(), json!(prize_name));
    merged.insert("status".into(), json!("completed"));
    merged.insert("finalizedAt".into(), json!(finalized_at));
    merged
}

fn map_slot(doc: Map<String, Value>) -> Option<InventorySlot> {
    let id = doc
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| document_id_from_map(&doc))?;

    Some(InventorySlot {
        id,
        location_id: doc.get("locationId")?.as_str()?.to_string(),
        prize_id: doc.get("prizeId")?.as_str()?.to_string(),
        total_quantity: i64_from_value(doc.get("totalQuantity")?)?,
        awarded_count: doc
            .get("awardedCount")
            .and_then(i64_from_value)
            .unwrap_or(0),
        updated_at: doc
            .get("updatedAt")
            .and_then(|v| crate::utils::firestore::millis_from_value(v)),
    })
}

fn i64_from_value(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|v| v as i64)),
        _ => None,
    }
}

fn map_claim_error(err: FirestoreError) -> ApiError {
    let msg = err.to_string();
    if msg.contains("INVENTORY_EXHAUSTED") {
        return ApiError::with_code(
            axum::http::StatusCode::CONFLICT,
            "INVENTORY_EXHAUSTED",
            "Prize inventory exhausted for this location.",
        );
    }
    if msg.contains("SPIN_TOKEN_ALREADY_USED") {
        return ApiError::with_code(
            axum::http::StatusCode::CONFLICT,
            "SPIN_TOKEN_ALREADY_USED",
            "This spin token has already been used.",
        );
    }
    ApiError::Internal(err.into())
}

pub fn inventory_view_to_json(view: &InventoryView) -> Value {
    json!({
        "locationId": view.location_id,
        "locationName": view.location_name,
        "prizeId": view.prize_id,
        "prizeName": view.prize_name,
        "totalQuantity": view.total_quantity,
        "awardedCount": view.awarded_count,
        "releasableNow": view.releasable_now,
        "remaining": view.remaining,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_slot_reads_integer_quantities() {
        let doc = Map::from_iter([
            ("id".into(), json!("loc1_prize1")),
            ("locationId".into(), json!("loc1")),
            ("prizeId".into(), json!("prize1")),
            ("totalQuantity".into(), json!(10)),
            ("awardedCount".into(), json!(2)),
        ]);

        let slot = map_slot(doc).expect("slot");
        assert_eq!(slot.total_quantity, 10);
        assert_eq!(slot.awarded_count, 2);
    }
}
