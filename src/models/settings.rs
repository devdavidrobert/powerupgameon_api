use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::utils::firestore::millis_now;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

const COLLECTION: &str = "settings";
const DOC_ID: &str = "general";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GameSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub challenge_start_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub challenge_end_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

pub struct SettingsModel;

impl SettingsModel {
    pub async fn get(state: &AppState) -> ApiResult<GameSettings> {
        let doc: Option<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .by_id_in(COLLECTION)
            .obj()
            .one(DOC_ID)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(map_settings(doc))
    }

    pub async fn upsert(state: &AppState, data: Map<String, Value>) -> ApiResult<GameSettings> {
        let mut payload = data;
        payload.insert("updatedAt".into(), json!(millis_now()));

        state
            .db
            .client
            .fluent()
            .update()
            .in_col(COLLECTION)
            .document_id(DOC_ID)
            .object(&payload)
            .execute::<Map<String, Value>>()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Self::get(state).await
    }

    pub async fn clear_timers(state: &AppState) -> ApiResult<()> {
        let payload = json!({
            "challengeStartTime": null,
            "challengeEndTime": null,
            "updatedAt": millis_now(),
        });
        state
            .db
            .client
            .fluent()
            .update()
            .in_col(COLLECTION)
            .document_id(DOC_ID)
            .object(&payload)
            .execute::<Map<String, Value>>()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
        Ok(())
    }
}

fn map_settings(doc: Option<Map<String, Value>>) -> GameSettings {
    let Some(doc) = doc else {
        return GameSettings::default();
    };
    GameSettings {
        challenge_start_time: doc
            .get("challengeStartTime")
            .and_then(|v| crate::utils::firestore::millis_from_value(v)),
        challenge_end_time: doc
            .get("challengeEndTime")
            .and_then(|v| crate::utils::firestore::millis_from_value(v)),
        updated_at: doc
            .get("updatedAt")
            .and_then(|v| crate::utils::firestore::millis_from_value(v)),
    }
}
