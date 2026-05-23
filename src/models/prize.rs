use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::utils::firestore::millis_now;
use firestore::FirestoreQueryDirection;
use serde_json::{json, Map, Value};

const COLLECTION: &str = "prizes";

pub struct PrizeModel;

impl PrizeModel {
    pub async fn find_all(state: &AppState) -> ApiResult<Vec<Map<String, Value>>> {
        let rows: Vec<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .from(COLLECTION)
            .order_by([("order", FirestoreQueryDirection::Ascending)])
            .obj::<Map<String, Value>>()
            .query()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(rows
            .into_iter()
            .map(|mut row: Map<String, Value>| {
                row.entry("isRealPrize")
                    .or_insert(json!(true));
                row
            })
            .collect())
    }

    pub async fn find_by_id(state: &AppState, id: &str) -> ApiResult<Option<Map<String, Value>>> {
        let doc: Option<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .by_id_in(COLLECTION)
            .obj::<Map<String, Value>>()
            .one(id)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(doc.map(|mut row: Map<String, Value>| {
            row.entry("isRealPrize").or_insert(json!(true));
            row
        }))
    }

    pub async fn create(state: &AppState, data: Map<String, Value>) -> ApiResult<Map<String, Value>> {
        let mut payload = data;
        payload.insert("createdAt".into(), json!(millis_now()));
        let id = uuid::Uuid::new_v4().to_string();

        state
            .db
            .client
            .fluent()
            .update()
            .in_col(COLLECTION)
            .document_id(&id)
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
        id: &str,
        data: Map<String, Value>,
    ) -> ApiResult<Map<String, Value>> {
        let mut payload = data;
        payload.insert("updatedAt".into(), json!(millis_now()));

        state
            .db
            .client
            .fluent()
            .update()
            .in_col(COLLECTION)
            .document_id(id)
            .object(&payload)
            .execute::<Map<String, Value>>()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Self::find_by_id(state, id)
            .await?
            .ok_or_else(|| ApiError::bad_request("Prize not found."))
    }

    pub async fn delete(state: &AppState, id: &str) -> ApiResult<()> {
        state
            .db
            .client
            .fluent()
            .delete()
            .from(COLLECTION)
            .document_id(id)
            .execute()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
        Ok(())
    }
}
