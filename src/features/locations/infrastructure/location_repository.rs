use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::infrastructure::CampaignPaths;
use crate::features::locations::domain::Location;
use crate::utils::firestore::millis_now;
use firestore::FirestoreQueryDirection;
use serde_json::{json, Map, Value};

const LOCATIONS_SUBCOL: &str = "locations";

pub struct LocationRepository;

impl LocationRepository {
    pub async fn find_all(state: &AppState, paths: &CampaignPaths) -> ApiResult<Vec<Location>> {
        let parent = paths.parent_str(&state.db.client)?;
        let rows: Vec<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .from(LOCATIONS_SUBCOL)
            .parent(parent)
            .order_by([("name", FirestoreQueryDirection::Ascending)])
            .obj()
            .query()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(rows
            .into_iter()
            .filter_map(|mut row| {
                if !row.contains_key("id") {
                    if let Some(id) = doc_id(&row) {
                        row.insert("id".into(), json!(id));
                    }
                }
                map_location(row)
            })
            .collect())
    }

    pub async fn find_by_id(
        state: &AppState,
        paths: &CampaignPaths,
        id: &str,
    ) -> ApiResult<Option<Location>> {
        let parent = paths.parent_str(&state.db.client)?;
        let doc: Option<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .by_id_in(LOCATIONS_SUBCOL)
            .parent(parent)
            .obj::<Map<String, Value>>()
            .one(id)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(doc.and_then(|mut row| {
            row.entry("id").or_insert_with(|| json!(id));
            map_location(row)
        }))
    }

    pub async fn create(
        state: &AppState,
        paths: &CampaignPaths,
        name: &str,
        center_lat: f64,
        center_lng: f64,
        radius_meters: f64,
        enabled: bool,
    ) -> ApiResult<Location> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = millis_now();
        let parent = paths.parent_str(&state.db.client)?;
        let payload = json!({
            "name": name,
            "centerLat": center_lat,
            "centerLng": center_lng,
            "radiusMeters": radius_meters,
            "enabled": enabled,
            "createdAt": now,
            "updatedAt": now,
        });

        state
            .db
            .client
            .fluent()
            .update()
            .in_col(LOCATIONS_SUBCOL)
            .document_id(&id)
            .parent(parent)
            .object(&payload)
            .execute::<Map<String, Value>>()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(Location {
            id,
            name: name.to_string(),
            center_lat,
            center_lng,
            radius_meters,
            enabled,
            created_at: Some(now),
            updated_at: Some(now),
        })
    }

    pub async fn update(
        state: &AppState,
        paths: &CampaignPaths,
        id: &str,
        data: Map<String, Value>,
    ) -> ApiResult<Location> {
        let parent = paths.parent_str(&state.db.client)?;
        let mut payload = data;
        payload.insert("updatedAt".into(), json!(millis_now()));

        state
            .db
            .client
            .fluent()
            .update()
            .in_col(LOCATIONS_SUBCOL)
            .document_id(id)
            .parent(parent)
            .object(&payload)
            .execute::<Map<String, Value>>()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Self::find_by_id(state, paths, id)
            .await?
            .ok_or_else(|| ApiError::bad_request("Location not found."))
    }

    pub async fn delete(state: &AppState, paths: &CampaignPaths, id: &str) -> ApiResult<()> {
        let parent = paths.parent_str(&state.db.client)?;
        state
            .db
            .client
            .fluent()
            .delete()
            .from(LOCATIONS_SUBCOL)
            .parent(parent)
            .document_id(id)
            .execute()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
        Ok(())
    }
}

pub fn map_location(doc: Map<String, Value>) -> Option<Location> {
    let id = doc
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| doc_id(&doc))?;

    Some(Location {
        id,
        name: doc.get("name")?.as_str()?.to_string(),
        center_lat: f64_from_value(doc.get("centerLat")?)?,
        center_lng: f64_from_value(doc.get("centerLng")?)?,
        radius_meters: f64_from_value(doc.get("radiusMeters")?)?,
        enabled: doc.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        created_at: doc
            .get("createdAt")
            .and_then(|v| crate::utils::firestore::millis_from_value(v)),
        updated_at: doc
            .get("updatedAt")
            .and_then(|v| crate::utils::firestore::millis_from_value(v)),
    })
}

fn doc_id(row: &Map<String, Value>) -> Option<String> {
    row.get("__name__")
        .and_then(|v| v.as_str())
        .map(|s| s.rsplit('/').next().unwrap_or(s).to_string())
}

fn f64_from_value(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64().or_else(|| n.as_i64().map(|v| v as f64)),
        _ => None,
    }
}

pub fn location_to_json(loc: &Location) -> Value {
    json!({
        "id": loc.id,
        "name": loc.name,
        "centerLat": loc.center_lat,
        "centerLng": loc.center_lng,
        "radiusMeters": loc.radius_meters,
        "enabled": loc.enabled,
        "createdAt": loc.created_at,
        "updatedAt": loc.updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_location_reads_document_id_field() {
        let doc = Map::from_iter([
            ("id".into(), json!("loc-1")),
            ("name".into(), json!("Nairobi CBD")),
            ("centerLat".into(), json!(-1.286389)),
            ("centerLng".into(), json!(36.817223)),
            ("radiusMeters".into(), json!(500)),
            ("enabled".into(), json!(true)),
        ]);

        let location = map_location(doc).expect("location");
        assert_eq!(location.id, "loc-1");
        assert_eq!(location.radius_meters, 500.0);
    }

    #[test]
    fn map_location_reads_firestore_name_path() {
        let doc = Map::from_iter([
            (
                "__name__".into(),
                json!("campaigns/c1/locations/loc-from-path"),
            ),
            ("name".into(), json!("Zone A")),
            ("centerLat".into(), json!(-1.28)),
            ("centerLng".into(), json!(36.81)),
            ("radiusMeters".into(), json!(250)),
        ]);

        let location = map_location(doc).expect("location");
        assert_eq!(location.id, "loc-from-path");
    }
}
