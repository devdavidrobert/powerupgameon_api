use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::utils::firestore::millis_now;
use firestore::FirestoreQueryDirection;
use once_cell::sync::Lazy;
use serde_json::{json, Map, Value};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const COLLECTION: &str = "questions";
const LIST_CACHE_TTL: Duration = Duration::from_secs(45);

static LIST_CACHE: Lazy<Mutex<Option<(Instant, Vec<Map<String, Value>>)>>> =
    Lazy::new(|| Mutex::new(None));

pub struct QuestionModel;

impl QuestionModel {
    pub fn invalidate_list_cache() {
        *LIST_CACHE.lock().unwrap() = None;
    }

    pub async fn find_all(state: &AppState) -> ApiResult<Vec<Map<String, Value>>> {
        {
            let cache = LIST_CACHE.lock().unwrap();
            if let Some((at, rows)) = cache.as_ref() {
                if at.elapsed() < LIST_CACHE_TTL {
                    return Ok(rows.clone());
                }
            }
        }

        let rows: Vec<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .from(COLLECTION)
            .order_by([("order", FirestoreQueryDirection::Ascending)])
            .obj()
            .query()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        *LIST_CACHE.lock().unwrap() = Some((Instant::now(), rows.clone()));
        Ok(rows)
    }

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
            .ok_or_else(|| ApiError::bad_request("Question not found."))
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
