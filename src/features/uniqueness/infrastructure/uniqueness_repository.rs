use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::infrastructure::CampaignPaths;
use crate::features::uniqueness::domain::{DeviceFingerprint, DeviceLockDoc, UNIQUENESS_SUBCOL};
use crate::utils::firestore::millis_now;
use firestore::errors::{
    BackoffError, FirestoreError, FirestoreInvalidParametersError,
    FirestoreInvalidParametersPublicDetails,
};
use serde_json::{json, Map, Value};
use std::future::Future;
use std::pin::Pin;

pub struct UniquenessRepository;

impl UniquenessRepository {
    /// Finds an existing device lock document (non-transactional path, e.g. for
    /// pre-checks or admin tools). Returns the raw map so callers can inspect
    /// playedAt etc.
    pub async fn find_device_lock(
        state: &AppState,
        paths: &CampaignPaths,
        device_id: &str,
    ) -> ApiResult<Option<Map<String, Value>>> {
        let parent = paths.parent_str(&state.db.client)?;
        let lock_id =
            crate::features::uniqueness::application::UniquenessService::device_lock_id(device_id);

        let doc: Option<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .by_id_in(UNIQUENESS_SUBCOL)
            .parent(parent)
            .obj()
            .one(&lock_id)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(doc)
    }

    /// Transaction-safe read helper intended to be called from inside a
    /// register_tx or similar Firestore transaction closure. Returns the raw
    /// document (or None) using the provided db handle.
    pub async fn find_device_lock_tx(
        db: &firestore::FirestoreDb,
        parent: &str,
        device_id: &str,
    ) -> Result<Option<Map<String, Value>>, BackoffError<FirestoreError>> {
        let lock_id =
            crate::features::uniqueness::application::UniquenessService::device_lock_id(device_id);
        crate::utils::firestore_tx::tx_get_optional(db, parent, UNIQUENESS_SUBCOL, &lock_id).await
    }

    /// Creates (or overwrites) a device lock document. Primarily used after a
    /// successful registration/play for non-tx paths, or for testing. In the
    /// normal registration flow the write is performed directly inside the
    /// transaction using the transaction handle.
    pub async fn create_device_lock(
        state: &AppState,
        paths: &CampaignPaths,
        device_id: &str,
        session_id: &str,
        ip: &str,
        user_agent: &str,
        fingerprint: Option<DeviceFingerprint>,
    ) -> ApiResult<()> {
        let parent = paths.parent_str(&state.db.client)?;
        let lock_id =
            crate::features::uniqueness::application::UniquenessService::device_lock_id(device_id);
        let now = millis_now();

        let doc = DeviceLockDoc {
            kind: "device_lock".to_string(),
            device_id: device_id.to_string(),
            session_id: session_id.to_string(),
            has_played: true,
            played_at: Some(now),
            registered_at: now,
            ip: ip.to_string(),
            user_agent: user_agent.to_string(),
            device_fingerprint: fingerprint,
        };

        state
            .db
            .client
            .fluent()
            .update()
            .in_col(UNIQUENESS_SUBCOL)
            .document_id(&lock_id)
            .parent(parent)
            .object(&doc)
            .execute::<Map<String, Value>>()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(())
    }

    /// Releases (deletes) the device lock for a given deviceId. Called from
    /// admin delete flow so a person can re-register after an admin correction.
    pub async fn release_device_lock(
        state: &AppState,
        paths: &CampaignPaths,
        device_id: &str,
    ) -> ApiResult<()> {
        let parent = paths.parent_str(&state.db.client)?;
        let lock_id =
            crate::features::uniqueness::application::UniquenessService::device_lock_id(device_id);

        // Best-effort delete; ignore not-found.
        let _ = state
            .db
            .client
            .fluent()
            .delete()
            .from(UNIQUENESS_SUBCOL)
            .parent(parent)
            .document_id(&lock_id)
            .execute()
            .await;

        Ok(())
    }

    /// Adds a device lock delete to an existing Firestore transaction.
    pub fn delete_device_lock_tx(
        db: &firestore::FirestoreDb,
        transaction: &mut firestore::FirestoreTransaction,
        parent: &str,
        device_id: &str,
    ) -> Result<(), BackoffError<FirestoreError>> {
        let lock_id =
            crate::features::uniqueness::application::UniquenessService::device_lock_id(device_id);
        db.fluent()
            .delete()
            .from(UNIQUENESS_SUBCOL)
            .parent(parent)
            .document_id(&lock_id)
            .add_to_transaction(transaction)
            .map_err(BackoffError::Permanent)?;
        Ok(())
    }

    /// Adds a device lock delete to an existing batch (used inside the
    /// registration delete batch together with name_lock and player docs).
    pub fn delete_device_lock_in_batch(
        db: &firestore::FirestoreDb,
        batch: &mut firestore::FirestoreBatch<'_, firestore::FirestoreSimpleBatchWriter>,
        parent: &str,
        device_id: &str,
    ) -> ApiResult<()> {
        let lock_id =
            crate::features::uniqueness::application::UniquenessService::device_lock_id(device_id);
        db.fluent()
            .delete()
            .from(UNIQUENESS_SUBCOL)
            .parent(parent)
            .document_id(&lock_id)
            .add_to_batch(batch)
            .map_err(|e| ApiError::Internal(e.into()))?;
        Ok(())
    }

    /// Helper that can be invoked from inside a transaction closure to write
    /// the device lock as part of the same atomic transaction that creates the
    /// player registration. Returns the BackoffError type expected by callers.
    pub fn write_device_lock_tx<'b>(
        db: &'b firestore::FirestoreDb,
        transaction: &'b mut firestore::FirestoreTransaction,
        parent: &str,
        device_id: &str,
        session_id: &str,
        ip: &str,
        user_agent: &str,
        fingerprint: Option<DeviceFingerprint>,
        now: i64,
    ) -> Pin<Box<dyn Future<Output = Result<(), BackoffError<FirestoreError>>> + Send + 'b>> {
        let device_id = device_id.to_string();
        let session_id = session_id.to_string();
        let ip = ip.to_string();
        let user_agent = user_agent.to_string();
        let parent = parent.to_string();

        Box::pin(async move {
            let lock_id =
                crate::features::uniqueness::application::UniquenessService::device_lock_id(
                    &device_id,
                );

            let payload = json!({
                "kind": "device_lock",
                "deviceId": device_id,
                "sessionId": session_id,
                "hasPlayed": true,
                "playedAt": now,
                "registeredAt": now,
                "ip": ip,
                "userAgent": user_agent,
                "deviceFingerprint": fingerprint,
            });

            db.fluent()
                .update()
                .in_col(UNIQUENESS_SUBCOL)
                .document_id(&lock_id)
                .parent(&parent)
                .object(&payload)
                .add_to_transaction(transaction)
                .map_err(BackoffError::Permanent)?;

            Ok(())
        })
    }
}

fn _tx_err(code: &str) -> BackoffError<FirestoreError> {
    BackoffError::Permanent(FirestoreError::InvalidParametersError(
        FirestoreInvalidParametersError::new(FirestoreInvalidParametersPublicDetails::new(
            code.to_string(),
            "code".to_string(),
        )),
    ))
}
