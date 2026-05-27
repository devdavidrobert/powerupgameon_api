use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::infrastructure::CampaignPaths;
use crate::features::inventory::domain::{
    merge_inventory_slot_fields, resolve_inventory_decrement, InventorySlot,
};
use crate::features::uniqueness::infrastructure::UniquenessRepository;
use crate::middleware::rate_limit::clear_campaign_player_rate_limits;
use crate::models::prize::PrizeModel;
use crate::models::registration::RegistrationModel;
use crate::models::submission::SubmissionModel;
use crate::utils::firestore::{document_id_from_map, millis_now};
use crate::utils::firestore_tx::tx_get_optional;
use firestore::errors::{BackoffError, FirestoreError};
use serde_json::Map;
use std::future::Future;
use std::pin::Pin;

const REGISTRATIONS_SUBCOL: &str = "registrations";
const SESSIONS_SUBCOL: &str = "sessions";
const SUBMISSIONS_SUBCOL: &str = "submissions";
const INVENTORY_SUBCOL: &str = "inventory";
const SPIN_TOKENS_SUBCOL: &str = "spin_tokens";
const RAFFLE_WINNERS_SUBCOL: &str = "raffle_winners";

pub struct PlayerSessionRepository;

impl PlayerSessionRepository {
    pub async fn delete_all(
        state: &AppState,
        paths: &CampaignPaths,
        session_id: &str,
    ) -> ApiResult<()> {
        let registration = RegistrationModel::find_by_id(state, paths, session_id).await?;
        let submission = SubmissionModel::find_by_id(state, paths, session_id).await?;
        let session = Self::find_session(state, paths, session_id).await?;
        let prizes = PrizeModel::find_all(state, paths).await?;

        let decrement = submission
            .as_ref()
            .and_then(|sub| resolve_inventory_decrement(sub, &prizes));
        let normalized_name = registration
            .as_ref()
            .and_then(|r| r.get("normalizedName").and_then(|v| v.as_str()))
            .or_else(|| {
                submission
                    .as_ref()
                    .and_then(|s| s.get("normalizedName").and_then(|v| v.as_str()))
            })
            .map(String::from);
        let device_id = registration
            .as_ref()
            .and_then(|r| r.get("deviceId").and_then(|v| v.as_str()))
            .or_else(|| session.as_ref().and_then(|s| s.get("deviceId").and_then(|v| v.as_str())))
            .map(String::from);

        let spin_token_ids = Self::find_spin_token_ids(state, paths, session_id).await?;
        let raffle_winner_ids = Self::find_raffle_winner_ids(state, paths, session_id).await?;
        let player_ip = resolve_player_ip(&registration, &submission);

        let db = state.db.client.clone();
        let parent = paths.parent_str(&state.db.client)?;
        let session_id = session_id.to_string();

        db.run_transaction(move |db, transaction| {
            delete_player_session_tx(
                db,
                transaction,
                parent.clone(),
                session_id.clone(),
                normalized_name.clone(),
                device_id.clone(),
                decrement.clone(),
                spin_token_ids.clone(),
                raffle_winner_ids.clone(),
            )
        })
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

        if let Some(ip) = player_ip {
            clear_campaign_player_rate_limits(
                &state.config,
                &state.redis,
                &paths.campaign_id,
                &ip,
            )
            .await;
        }

        Ok(())
    }

    async fn find_session(
        state: &AppState,
        paths: &CampaignPaths,
        session_id: &str,
    ) -> ApiResult<Option<Map<String, serde_json::Value>>> {
        let parent = paths.parent_str(&state.db.client)?;
        state
            .db
            .client
            .fluent()
            .select()
            .by_id_in(SESSIONS_SUBCOL)
            .parent(parent)
            .obj()
            .one(session_id)
            .await
            .map_err(|e| ApiError::Internal(e.into()))
    }

    async fn find_spin_token_ids(
        state: &AppState,
        paths: &CampaignPaths,
        session_id: &str,
    ) -> ApiResult<Vec<String>> {
        let parent = paths.parent_str(&state.db.client)?;
        let docs: Vec<Map<String, serde_json::Value>> = state
            .db
            .client
            .fluent()
            .select()
            .from(SPIN_TOKENS_SUBCOL)
            .parent(parent)
            .filter(|q| q.field("sessionId").eq(session_id))
            .obj()
            .query()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(docs
            .iter()
            .filter_map(document_id_from_map)
            .collect())
    }

    async fn find_raffle_winner_ids(
        state: &AppState,
        paths: &CampaignPaths,
        session_id: &str,
    ) -> ApiResult<Vec<String>> {
        let parent = paths.parent_str(&state.db.client)?;
        let docs: Vec<Map<String, serde_json::Value>> = state
            .db
            .client
            .fluent()
            .select()
            .from(RAFFLE_WINNERS_SUBCOL)
            .parent(parent)
            .filter(|q| q.field("originalSubmissionId").eq(session_id))
            .obj()
            .query()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(docs
            .iter()
            .filter_map(document_id_from_map)
            .collect())
    }
}

fn resolve_player_ip(
    registration: &Option<Map<String, serde_json::Value>>,
    submission: &Option<Map<String, serde_json::Value>>,
) -> Option<String> {
    registration
        .as_ref()
        .and_then(|r| r.get("ip").and_then(|v| v.as_str()))
        .or_else(|| {
            submission
                .as_ref()
                .and_then(|s| s.get("ip").and_then(|v| v.as_str()))
        })
        .map(str::trim)
        .filter(|ip| !ip.is_empty())
        .map(String::from)
}

fn delete_player_session_tx<'b>(
    db: firestore::FirestoreDb,
    transaction: &'b mut firestore::FirestoreTransaction,
    parent: String,
    session_id: String,
    normalized_name: Option<String>,
    device_id: Option<String>,
    decrement: Option<(String, String)>,
    spin_token_ids: Vec<String>,
    raffle_winner_ids: Vec<String>,
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
                    let now = millis_now();
                    let payload = merge_inventory_slot_fields(&slot_doc, awarded - 1, now);
                    db.fluent()
                        .update()
                        .in_col(INVENTORY_SUBCOL)
                        .document_id(&slot_id)
                        .parent(parent.as_str())
                        .object(&payload)
                        .add_to_transaction(transaction)
                        .map_err(BackoffError::Permanent)?;
                }
            }
        }

        delete_doc_tx(&db, transaction, &parent, SUBMISSIONS_SUBCOL, &session_id)?;
        delete_doc_tx(&db, transaction, &parent, SESSIONS_SUBCOL, &session_id)?;
        delete_doc_tx(&db, transaction, &parent, REGISTRATIONS_SUBCOL, &session_id)?;

        if let Some(normalized) = normalized_name {
            delete_doc_tx(
                &db,
                transaction,
                &parent,
                REGISTRATIONS_SUBCOL,
                &format!("name_{normalized}"),
            )?;
        }

        if let Some(device_id) = device_id {
            UniquenessRepository::delete_device_lock_tx(
                &db,
                transaction,
                parent.as_str(),
                &device_id,
            )?;
        }

        for token_id in spin_token_ids {
            delete_doc_tx(&db, transaction, &parent, SPIN_TOKENS_SUBCOL, &token_id)?;
        }

        for winner_id in raffle_winner_ids {
            delete_doc_tx(
                &db,
                transaction,
                &parent,
                RAFFLE_WINNERS_SUBCOL,
                &winner_id,
            )?;
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
