use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::utils::firestore::millis_now;
use firestore::errors::FirestoreError;
use serde_json::{json, Map, Value};

const COLLECTION: &str = "spin_tokens";
const RECORD_TTL_MS: i64 = 2 * 60 * 60 * 1000;

pub struct SpinTokenModel;

impl SpinTokenModel {
    pub async fn is_consumed(state: &AppState, fingerprint: &str) -> ApiResult<bool> {
        let doc: Option<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .by_id_in(COLLECTION)
            .obj()
            .one(fingerprint)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
        Ok(doc.is_some())
    }

    pub async fn consume_if_fresh(
        state: &AppState,
        fingerprint: &str,
        session_id: &str,
    ) -> ApiResult<bool> {
        let now = millis_now();
        let payload = json!({
            "sessionId": session_id,
            "consumedAt": now,
            "expiresAt": now + RECORD_TTL_MS,
        });

        match state
            .db
            .client
            .fluent()
            .insert()
            .into(COLLECTION)
            .document_id(fingerprint)
            .object(&payload)
            .execute::<Map<String, Value>>()
            .await
        {
            Ok(_) => Ok(true),
            Err(FirestoreError::DataConflictError(_)) => Ok(false),
            Err(err) => Err(ApiError::Internal(err.into())),
        }
    }

    pub async fn mark_consumed(
        state: &AppState,
        fingerprint: &str,
        session_id: &str,
    ) -> ApiResult<()> {
        let now = millis_now();
        state
            .db
            .client
            .fluent()
            .update()
            .in_col(COLLECTION)
            .document_id(fingerprint)
            .object(&json!({
                "sessionId": session_id,
                "consumedAt": now,
                "expiresAt": now + RECORD_TTL_MS,
            }))
            .execute::<Map<String, Value>>()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
        Ok(())
    }
}
