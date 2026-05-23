use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::infrastructure::CampaignPaths;
use crate::utils::firestore::millis_now;
use firestore::FirestoreQueryDirection;
use serde_json::{json, Map, Value};

const PRIZES_SUBCOL: &str = "prizes";

pub struct PrizeModel;

impl PrizeModel {
    pub async fn find_all(
        state: &AppState,
        paths: &CampaignPaths,
    ) -> ApiResult<Vec<Map<String, Value>>> {
        let parent = paths.parent_str(&state.db.client)?;
        let rows: Vec<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .from(PRIZES_SUBCOL)
            .parent(parent)
            .order_by([("order", FirestoreQueryDirection::Ascending)])
            .obj::<Map<String, Value>>()
            .query()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(rows
            .into_iter()
            .map(|mut row| {
                row.entry("isRealPrize").or_insert(json!(true));
                if !row.contains_key("id") {
                    if let Some(id) = doc_id(&row) {
                        row.insert("id".into(), json!(id));
                    }
                }
                row
            })
            .collect())
    }

    pub async fn find_by_id(
        state: &AppState,
        paths: &CampaignPaths,
        id: &str,
    ) -> ApiResult<Option<Map<String, Value>>> {
        let parent = paths.parent_str(&state.db.client)?;
        let doc: Option<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .by_id_in(PRIZES_SUBCOL)
            .parent(parent)
            .obj::<Map<String, Value>>()
            .one(id)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(doc.map(|mut row| {
            row.entry("isRealPrize").or_insert(json!(true));
            row.insert("id".into(), json!(id));
            row
        }))
    }

    pub async fn create(
        state: &AppState,
        paths: &CampaignPaths,
        data: Map<String, Value>,
    ) -> ApiResult<Map<String, Value>> {
        let parent = paths.parent_str(&state.db.client)?;
        let mut payload = data;
        payload.insert("createdAt".into(), json!(millis_now()));
        let id = uuid::Uuid::new_v4().to_string();

        state
            .db
            .client
            .fluent()
            .update()
            .in_col(PRIZES_SUBCOL)
            .document_id(&id)
            .parent(parent)
            .object(&payload)
            .execute::<Map<String, Value>>()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        let mut result = payload;
        result.insert("id".into(), json!(id));
        Ok(result)
    }

    pub async fn update(
        state: &AppState,
        paths: &CampaignPaths,
        id: &str,
        data: Map<String, Value>,
    ) -> ApiResult<Map<String, Value>> {
        let parent = paths.parent_str(&state.db.client)?;
        let mut payload = data;
        payload.insert("updatedAt".into(), json!(millis_now()));

        state
            .db
            .client
            .fluent()
            .update()
            .in_col(PRIZES_SUBCOL)
            .document_id(id)
            .parent(parent)
            .object(&payload)
            .execute::<Map<String, Value>>()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Self::find_by_id(state, paths, id)
            .await?
            .ok_or_else(|| ApiError::bad_request("Prize not found."))
    }

    pub async fn delete(state: &AppState, paths: &CampaignPaths, id: &str) -> ApiResult<()> {
        let parent = paths.parent_str(&state.db.client)?;
        state
            .db
            .client
            .fluent()
            .delete()
            .from(PRIZES_SUBCOL)
            .parent(parent)
            .document_id(id)
            .execute()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
        Ok(())
    }
}

fn doc_id(row: &Map<String, Value>) -> Option<String> {
    row.get("__name__")
        .and_then(|v| v.as_str())
        .map(|s| s.rsplit('/').next().unwrap_or(s).to_string())
        .or_else(|| row.get("id").and_then(|v| v.as_str()).map(String::from))
}
