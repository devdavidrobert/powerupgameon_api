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

        Ok(rows.into_iter().filter_map(map_location).collect())
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
            .obj()
            .one(id)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(doc.and_then(map_location))
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

        Self::find_by_id(state, paths, &id)
            .await?
            .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("Location create failed")))
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
        .get("__name__")
        .and_then(|v| v.as_str())
        .map(|s| s.rsplit('/').next().unwrap_or(s).to_string())
        .or_else(|| doc.get("id").and_then(|v| v.as_str()).map(String::from))?;

    Some(Location {
        id,
        name: doc.get("name")?.as_str()?.to_string(),
        center_lat: doc.get("centerLat")?.as_f64()?,
        center_lng: doc.get("centerLng")?.as_f64()?,
        radius_meters: doc.get("radiusMeters")?.as_f64()?,
        enabled: doc.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        created_at: doc
            .get("createdAt")
            .and_then(|v| crate::utils::firestore::millis_from_value(v)),
        updated_at: doc
            .get("updatedAt")
            .and_then(|v| crate::utils::firestore::millis_from_value(v)),
    })
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
