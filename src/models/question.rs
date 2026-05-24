use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::infrastructure::CampaignPaths;
use crate::utils::firestore::{document_id_from_map, millis_now};
use dashmap::DashMap;
use firestore::FirestoreQueryDirection;
use once_cell::sync::Lazy;
use serde_json::{json, Map, Value};
use std::time::{Duration, Instant};

const QUESTIONS_SUBCOL: &str = "questions";
const LIST_CACHE_TTL: Duration = Duration::from_secs(45);

static LIST_CACHE: Lazy<DashMap<String, (Instant, Vec<Map<String, Value>>)>> =
    Lazy::new(DashMap::new);

pub struct QuestionModel;

impl QuestionModel {
    pub fn invalidate_list_cache(campaign_id: &str) {
        LIST_CACHE.remove(campaign_id);
    }

    pub async fn find_all(
        state: &AppState,
        paths: &CampaignPaths,
    ) -> ApiResult<Vec<Map<String, Value>>> {
        if let Some(entry) = LIST_CACHE.get(&paths.campaign_id) {
            if entry.0.elapsed() < LIST_CACHE_TTL {
                return Ok(entry.1.clone());
            }
        }

        let parent = paths.parent_str(&state.db.client)?;
        let rows: Vec<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .from(QUESTIONS_SUBCOL)
            .parent(parent)
            .order_by([("order", FirestoreQueryDirection::Ascending)])
            .obj()
            .query()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        let rows: Vec<Map<String, Value>> = rows
            .into_iter()
            .map(|mut row| {
                if let Some(id) = document_id_from_map(&row) {
                    row.insert("id".into(), json!(id));
                }
                row
            })
            .collect();

        LIST_CACHE.insert(paths.campaign_id.clone(), (Instant::now(), rows.clone()));
        Ok(rows)
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
            .by_id_in(QUESTIONS_SUBCOL)
            .parent(parent)
            .obj()
            .one(id)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(doc.map(|mut row| {
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
            .in_col(QUESTIONS_SUBCOL)
            .document_id(&id)
            .parent(parent)
            .object(&payload)
            .execute::<Map<String, Value>>()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Self::invalidate_list_cache(&paths.campaign_id);
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
            .in_col(QUESTIONS_SUBCOL)
            .document_id(id)
            .parent(parent)
            .object(&payload)
            .execute::<Map<String, Value>>()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Self::invalidate_list_cache(&paths.campaign_id);
        Self::find_by_id(state, paths, id)
            .await?
            .ok_or_else(|| ApiError::bad_request("Question not found."))
    }

    pub async fn delete(state: &AppState, paths: &CampaignPaths, id: &str) -> ApiResult<()> {
        let parent = paths.parent_str(&state.db.client)?;
        state
            .db
            .client
            .fluent()
            .delete()
            .from(QUESTIONS_SUBCOL)
            .parent(parent)
            .document_id(id)
            .execute()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
        Self::invalidate_list_cache(&paths.campaign_id);
        Ok(())
    }
}
