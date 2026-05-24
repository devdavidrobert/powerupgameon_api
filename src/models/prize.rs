use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::infrastructure::CampaignPaths;
use crate::utils::firestore::{document_id_from_map, millis_now};
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
                    if let Some(id) = document_id_from_map(&row) {
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
        if id.trim().is_empty() {
            return Ok(None);
        }

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
        payload.insert("id".into(), json!(id));

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;

    #[test]
    fn find_all_rows_get_id_from_firestore_metadata() {
        let row = Map::from_iter([
            ("_firestore_id".into(), json!("prize-abc")),
            ("name".into(), json!("Steam 500ml")),
            ("order".into(), json!(1)),
        ]);

        let id = document_id_from_map(&row).expect("id");
        assert_eq!(id, "prize-abc");
    }
}
