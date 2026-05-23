use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::utils::firestore::millis_now;
use firestore::errors::{
    BackoffError, FirestoreError, FirestoreInvalidParametersError,
    FirestoreInvalidParametersPublicDetails,
};
use firestore::FirestoreQueryDirection;
use serde_json::{json, Map, Value};
use std::future::Future;
use std::pin::Pin;

const COLLECTION: &str = "registrations";
const SESSIONS_COLLECTION: &str = "sessions";

pub struct RegistrationModel;

impl RegistrationModel {
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

    pub async fn find_player_page(
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
            .filter(|q| q.field("kind").eq("player"))
            .order_by([
                ("registeredAt", FirestoreQueryDirection::Descending),
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

    pub async fn register(
        state: &AppState,
        session_id: &str,
        full_name: &str,
        normalized_name: &str,
        ip: &str,
        user_agent: &str,
    ) -> ApiResult<()> {
        let db = state.db.client.clone();
        let session_id = session_id.to_string();
        let full_name_upper = full_name.to_uppercase();
        let normalized_name = normalized_name.to_string();
        let name_ref = format!("name_{normalized_name}");
        let ip = ip.to_string();
        let user_agent = user_agent.to_string();
        let now = millis_now();

        db.run_transaction(move |db, transaction| {
            register_tx(
                db,
                transaction,
                session_id.clone(),
                full_name_upper.clone(),
                normalized_name.clone(),
                name_ref.clone(),
                ip.clone(),
                user_agent.clone(),
                now,
            )
        })
        .await
        .map_err(map_registration_error)?;

        Ok(())
    }

    pub async fn delete(state: &AppState, id: &str) -> ApiResult<()> {
        let reg = Self::find_by_id(state, id).await?;
        let Some(reg) = reg else {
            return Ok(());
        };

        let writer = state.db.batch_writer().await.map_err(|e| ApiError::Internal(e.into()))?;
        let mut batch = writer.new_batch();

        db_delete(&state.db.client, &mut batch, COLLECTION, id)?;
        if let Some(normalized) = reg.get("normalizedName").and_then(|v| v.as_str()) {
            db_delete(
                &state.db.client,
                &mut batch,
                COLLECTION,
                &format!("name_{normalized}"),
            )?;
        }

        batch.write().await.map_err(|e| ApiError::Internal(e.into()))?;
        Ok(())
    }
}

fn register_tx<'b>(
    db: firestore::FirestoreDb,
    transaction: &'b mut firestore::FirestoreTransaction,
    session_id: String,
    full_name_upper: String,
    normalized_name: String,
    name_ref: String,
    ip: String,
    user_agent: String,
    now: i64,
) -> Pin<Box<dyn Future<Output = Result<(), BackoffError<FirestoreError>>> + Send + 'b>> {
    Box::pin(async move {
        let name_doc: Option<Map<String, Value>> = db
            .fluent()
            .select()
            .by_id_in(COLLECTION)
            .obj()
            .one(&name_ref)
            .await
            .map_err(BackoffError::Permanent)?;

        if name_doc.is_some() {
            return Err(tx_err("NAME_TAKEN"));
        }

        let session_doc: Option<Map<String, Value>> = db
            .fluent()
            .select()
            .by_id_in(SESSIONS_COLLECTION)
            .obj()
            .one(&session_id)
            .await
            .map_err(BackoffError::Permanent)?;

        if let Some(session) = session_doc {
            if session.get("hasPlayed").and_then(|v| v.as_bool()) == Some(true) {
                if let Some(played_at) = session
                    .get("playedAt")
                    .and_then(|v| crate::utils::firestore::millis_from_value(v))
                {
                    let hours = (millis_now() - played_at) as f64 / (1000.0 * 60.0 * 60.0);
                    if hours < 12.0 {
                        return Err(tx_err("SESSION_COOLDOWN"));
                    }
                } else {
                    return Err(tx_err("SESSION_COOLDOWN"));
                }
            }
        }

        db.fluent()
            .update()
            .in_col(SESSIONS_COLLECTION)
            .document_id(&session_id)
            .object(&json!({
                "fullName": full_name_upper,
                "sessionId": session_id,
                "hasPlayed": true,
                "playedAt": now,
            }))
            .add_to_transaction(transaction)
            .map_err(BackoffError::Permanent)?;

        db.fluent()
            .update()
            .in_col(COLLECTION)
            .document_id(&name_ref)
            .object(&json!({
                "kind": "name_lock",
                "blocked": true,
                "fullName": full_name_upper,
                "normalizedName": normalized_name,
                "ip": ip,
                "userAgent": user_agent,
                "registeredAt": now,
            }))
            .add_to_transaction(transaction)
            .map_err(BackoffError::Permanent)?;

        db.fluent()
            .update()
            .in_col(COLLECTION)
            .document_id(&session_id)
            .object(&json!({
                "kind": "player",
                "sessionId": session_id,
                "fullName": full_name_upper,
                "normalizedName": normalized_name,
                "ip": ip,
                "userAgent": user_agent,
                "registeredAt": now,
            }))
            .add_to_transaction(transaction)
            .map_err(BackoffError::Permanent)?;

        Ok(())
    })
}

fn tx_err(code: &str) -> BackoffError<FirestoreError> {
    BackoffError::Permanent(FirestoreError::InvalidParametersError(
        FirestoreInvalidParametersError::new(FirestoreInvalidParametersPublicDetails::new(
            code.to_string(),
            "code".to_string(),
        )),
    ))
}

fn db_delete(
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

fn map_registration_error(err: FirestoreError) -> ApiError {
    let msg = err.to_string();
    if msg.contains("NAME_TAKEN") {
        return ApiError::WithStatus {
            status: axum::http::StatusCode::CONFLICT,
            message: "The name has already been registered.".into(),
            code: Some("NAME_TAKEN".into()),
        };
    }
    if msg.contains("SESSION_COOLDOWN") {
        return ApiError::WithStatus {
            status: axum::http::StatusCode::CONFLICT,
            message: "You have already played. Please try again next time!".into(),
            code: Some("SESSION_COOLDOWN".into()),
        };
    }
    ApiError::Internal(err.into())
}
