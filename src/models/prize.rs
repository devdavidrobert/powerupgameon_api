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
            .obj()
            .query()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(rows.into_iter().map(attach_prize_document_id).collect())
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
            .obj()
            .one(id)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(doc.map(|mut row| {
            row.insert("id".into(), json!(id));
            attach_prize_document_id(row)
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
        let Some(existing) = Self::find_by_id(state, paths, id).await? else {
            return Err(ApiError::bad_request("Prize not found."));
        };

        let parent = paths.parent_str(&state.db.client)?;
        let payload = merge_prize_fields(&existing, data, millis_now());

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

fn attach_prize_document_id(mut row: Map<String, Value>) -> Map<String, Value> {
    row.entry("isRealPrize".to_string()).or_insert(json!(true));
    if let Some(doc_id) = document_id_from_map(&row) {
        row.insert("id".into(), json!(doc_id));
    }
    row
}

/// Firestore `.update().object()` replaces the whole document in our client — merge fields
/// so name, order, and isRealPrize survive partial updates (e.g. image-only uploads).
fn merge_prize_fields(
    existing: &Map<String, Value>,
    updates: Map<String, Value>,
    updated_at: i64,
) -> Map<String, Value> {
    let mut merged: Map<String, Value> = existing
        .iter()
        .filter(|(k, _)| !k.starts_with('_'))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    for (key, value) in updates {
        if key == "updatedAt" {
            continue;
        }
        merged.insert(key, value);
    }

    merged.insert("updatedAt".into(), json!(updated_at));
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;

    #[test]
    fn attach_prize_document_id_uses_firestore_metadata_when_id_missing() {
        let row = Map::from_iter([
            ("_firestore_id".into(), json!("prize-abc")),
            ("name".into(), json!("Steam 500ml")),
            ("order".into(), json!(1)),
        ]);

        let attached = attach_prize_document_id(row);
        assert_eq!(attached.get("id").and_then(|v| v.as_str()), Some("prize-abc"));
        assert_eq!(
            attached.get("isRealPrize").and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn attach_prize_document_id_overwrites_blank_id_with_firestore_metadata() {
        let row = Map::from_iter([
            ("id".into(), json!("")),
            ("_firestore_id".into(), json!("prize-abc")),
            ("name".into(), json!("Steam 500ml")),
        ]);

        let attached = attach_prize_document_id(row);
        assert_eq!(attached.get("id").and_then(|v| v.as_str()), Some("prize-abc"));
    }

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

    #[test]
    fn merge_prize_fields_preserves_existing_fields_on_partial_update() {
        let existing = Map::from_iter([
            ("id".into(), json!("p1")),
            ("name".into(), json!("Steam Can")),
            ("order".into(), json!(1)),
            ("isRealPrize".into(), json!(true)),
            ("createdAt".into(), json!(1_700_000_000_000_i64)),
        ]);

        let merged = merge_prize_fields(
            &existing,
            Map::from_iter([("imageUrl".into(), json!("https://cdn.example/p.png"))]),
            1_700_000_000_100,
        );

        assert_eq!(merged.get("name").and_then(|v| v.as_str()), Some("Steam Can"));
        assert_eq!(merged.get("order").and_then(|v| v.as_i64()), Some(1));
        assert_eq!(
            merged.get("isRealPrize").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            merged.get("imageUrl").and_then(|v| v.as_str()),
            Some("https://cdn.example/p.png")
        );
        assert_eq!(
            merged.get("updatedAt").and_then(|v| v.as_i64()),
            Some(1_700_000_000_100)
        );
    }
}
