use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::domain::GeoEnforcement;
use crate::features::campaigns::infrastructure::CampaignPaths;
use crate::features::locations::application::{GeoService, IpGeoService};
use crate::features::locations::domain::{
    GeoPoint, GeoStatus, GeoValidationResult, IpGeoCrossCheck, IpGeoLookup,
};
use crate::features::locations::infrastructure::{IpApiProvider, LocationRepository};
use crate::features::uniqueness::application::UniquenessService;
use crate::features::uniqueness::infrastructure::UniquenessRepository;
use crate::utils::firestore::{
    build_page_cursor, millis_now, start_after_cursor,
};
use crate::utils::firestore_tx::tx_get_optional;
use firestore::errors::{
    BackoffError, FirestoreError, FirestoreInvalidParametersError,
    FirestoreInvalidParametersPublicDetails,
};
use firestore::FirestoreQueryDirection;
use serde_json::{json, Map, Value};
use std::future::Future;
use std::pin::Pin;

const REGISTRATIONS_SUBCOL: &str = "registrations";
const SESSIONS_SUBCOL: &str = "sessions";

pub struct RegistrationModel;

pub struct RegistrationInput {
    pub session_id: String,
    pub full_name: String,
    pub normalized_name: String,
    pub ip: String,
    pub user_agent: String,
    pub lat: f64,
    pub lng: f64,
    pub location_id: Option<String>,
    pub geo_status: GeoStatus,
    pub ip_lat: Option<f64>,
    pub ip_lng: Option<f64>,
    pub ip_geo_status: Option<String>,
    /// Stable device identifier from the client (primary anti-dupe signal).
    pub device_id: Option<String>,
    /// Raw device fingerprint payload (lightweight signals) for storage and correlation.
    pub device_fingerprint: Option<serde_json::Value>,
}

pub struct GeoResolveOutput {
    pub location_id: Option<String>,
    pub geo_status: GeoStatus,
    pub ip_lat: Option<f64>,
    pub ip_lng: Option<f64>,
    pub ip_geo_status: Option<String>,
}

impl RegistrationModel {
    pub async fn find_by_id(
        state: &AppState,
        paths: &CampaignPaths,
        id: &str,
    ) -> ApiResult<Option<Map<String, Value>>> {
        let parent = paths.parent_str(&state.db.client)?;
        state
            .db
            .client
            .fluent()
            .select()
            .by_id_in(REGISTRATIONS_SUBCOL)
            .parent(parent)
            .obj()
            .one(id)
            .await
            .map_err(|e| ApiError::Internal(e.into()))
    }

    pub async fn find_player_page(
        state: &AppState,
        paths: &CampaignPaths,
        limit: usize,
        cursor: Option<Map<String, Value>>,
    ) -> ApiResult<(Vec<Map<String, Value>>, Option<Map<String, Value>>, bool)> {
        let parent = paths.parent_str(&state.db.client)?;
        let cap = limit.clamp(1, 200);
        let query_limit = (cap + 1) as u32;

        let mut query = state
            .db
            .client
            .fluent()
            .select()
            .from(REGISTRATIONS_SUBCOL)
            .parent(parent.as_str())
            .filter(|q| q.field("kind").eq("player"))
            .order_by([
                ("registeredAt", FirestoreQueryDirection::Descending),
                ("__name__", FirestoreQueryDirection::Descending),
            ]);

        if let Some(ref cursor_map) = cursor {
            query = query.start_at(start_after_cursor(cursor_map, "registeredAt")?);
        }

        let mut items: Vec<Map<String, Value>> = query
            .limit(query_limit)
            .obj()
            .query()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        let has_more = items.len() > cap;
        if has_more {
            items.truncate(cap);
        }
        let next_cursor = if has_more {
            items
                .last()
                .and_then(|row| build_page_cursor(row, "registeredAt", parent.as_str(), REGISTRATIONS_SUBCOL))
        } else {
            None
        };
        Ok((items, next_cursor, has_more))
    }

    pub async fn resolve_geo(
        state: &AppState,
        paths: &CampaignPaths,
        geo_enforcement: GeoEnforcement,
        lat: f64,
        lng: f64,
        client_ip: &str,
    ) -> ApiResult<GeoResolveOutput> {
        GeoService::validate_coordinates(lat, lng).map_err(ApiError::bad_request)?;

        let locations = LocationRepository::find_all(state, paths).await?;
        let point = GeoPoint { lat, lng };
        let gps_result = GeoService::resolve_location(&point, &locations);

        let (location_id, geo_status) = match &gps_result {
            GeoValidationResult::Matched { location_id } => {
                (Some(location_id.clone()), GeoStatus::Valid)
            }
            GeoValidationResult::NoZonesConfigured => (None, GeoStatus::NoZones),
            GeoValidationResult::OutsideZones => {
                return Err(ApiError::with_code(
                    axum::http::StatusCode::FORBIDDEN,
                    "GEO_OUTSIDE_ZONE",
                    "You are outside the allowed participation zones for this campaign.",
                ));
            }
        };

        let mut ip_lat = None;
        let mut ip_lng = None;
        let mut ip_geo_status = None;

        let has_enabled_zones = locations.iter().any(|l| l.enabled);
        if state.config.ip_geo_enabled
            && has_enabled_zones
            && IpGeoService::is_public_ip(client_ip)
        {
            let ip_lookup =
                IpApiProvider::lookup(client_ip, state.config.ip_geo_api_url.as_deref()).await;

            if let Some(coords) = match &ip_lookup {
                IpGeoLookup::Found(p) => Some(*p),
                _ => None,
            } {
                ip_lat = Some(coords.lat);
                ip_lng = Some(coords.lng);
            }

            match IpGeoService::cross_check_gps_and_ip(
                &point,
                &gps_result,
                ip_lookup,
                &locations,
                state.config.ip_geo_max_distance_km,
            ) {
                IpGeoCrossCheck::Pass => {
                    ip_geo_status = Some("ok".into());
                }
                IpGeoCrossCheck::Skipped => {
                    tracing::warn!(
                        client_ip = %client_ip,
                        "ip_geo_check_skipped"
                    );
                }
                IpGeoCrossCheck::Mismatch => {
                    ip_geo_status = Some("mismatch".into());
                    if geo_enforcement == GeoEnforcement::Reject {
                        return Err(ApiError::with_code(
                            axum::http::StatusCode::FORBIDDEN,
                            "GEO_IP_MISMATCH",
                            "Your location could not be verified. Please disable VPN or location spoofing and try again.",
                        ));
                    }
                }
            }
        }

        Ok(GeoResolveOutput {
            location_id,
            geo_status,
            ip_lat,
            ip_lng,
            ip_geo_status,
        })
    }

    pub async fn register(
        state: &AppState,
        paths: &CampaignPaths,
        input: RegistrationInput,
    ) -> ApiResult<()> {
        let parent = paths.parent_str(&state.db.client)?;
        let db = state.db.client.clone();
        let session_id = input.session_id.clone();
        let full_name_upper = input.full_name.to_uppercase();
        let normalized_name = input.normalized_name.clone();
        let name_ref = format!("name_{normalized_name}");
        let ip = input.ip.clone();
        let user_agent = input.user_agent.clone();
        let now = millis_now();
        let location_id = input.location_id.clone();
        let geo_status = input.geo_status;
        let lat = input.lat;
        let lng = input.lng;
        let ip_lat = input.ip_lat;
        let ip_lng = input.ip_lng;
        let ip_geo_status = input.ip_geo_status.clone();
        let device_id = input.device_id.clone();
        let device_fingerprint = input.device_fingerprint.clone();

        db.run_transaction(move |db, transaction| {
            register_tx(
                db,
                transaction,
                parent.clone(),
                session_id.clone(),
                full_name_upper.clone(),
                normalized_name.clone(),
                name_ref.clone(),
                ip.clone(),
                user_agent.clone(),
                now,
                location_id.clone(),
                geo_status,
                lat,
                lng,
                ip_lat,
                ip_lng,
                ip_geo_status.clone(),
                device_id.clone(),
                device_fingerprint.clone(),
            )
        })
        .await
        .map_err(map_registration_error)?;

        Ok(())
    }

    pub async fn delete(state: &AppState, paths: &CampaignPaths, id: &str) -> ApiResult<()> {
        let reg = Self::find_by_id(state, paths, id).await?;
        let Some(reg) = reg else {
            return Ok(());
        };

        let parent = paths.parent_str(&state.db.client)?;
        let writer = state.db.batch_writer().await.map_err(|e| ApiError::Internal(e.into()))?;
        let mut batch = writer.new_batch();

        db_delete(&state.db.client, &mut batch, parent.as_str(), REGISTRATIONS_SUBCOL, id)?;
        if let Some(normalized) = reg.get("normalizedName").and_then(|v| v.as_str()) {
            db_delete(
                &state.db.client,
                &mut batch,
                parent.as_str(),
                REGISTRATIONS_SUBCOL,
                &format!("name_{normalized}"),
            )?;
        }

        // Release the device lock (if present on this registration) so the person
        // can re-enter after an admin correction (mirrors name_lock cleanup).
        if let Some(device_id) = reg.get("deviceId").and_then(|v| v.as_str()) {
            // Best-effort; do not fail the whole delete if the lock row is already gone.
            let _ = UniquenessRepository::delete_device_lock_in_batch(
                &state.db.client,
                &mut batch,
                parent.as_str(),
                device_id,
            );
        }

        batch.write().await.map_err(|e| ApiError::Internal(e.into()))?;
        Ok(())
    }
}

fn register_tx<'b>(
    db: firestore::FirestoreDb,
    transaction: &'b mut firestore::FirestoreTransaction,
    parent: String,
    session_id: String,
    full_name_upper: String,
    normalized_name: String,
    name_ref: String,
    ip: String,
    user_agent: String,
    now: i64,
    location_id: Option<String>,
    geo_status: GeoStatus,
    lat: f64,
    lng: f64,
    ip_lat: Option<f64>,
    ip_lng: Option<f64>,
    ip_geo_status: Option<String>,
    device_id: Option<String>,
    device_fingerprint: Option<serde_json::Value>,
) -> Pin<Box<dyn Future<Output = Result<(), BackoffError<FirestoreError>>> + Send + 'b>> {
    Box::pin(async move {
        let name_doc = tx_get_optional(&db, parent.as_str(), REGISTRATIONS_SUBCOL, &name_ref).await?;

        if name_doc.is_some() {
            return Err(tx_err("NAME_TAKEN"));
        }

        let session_doc =
            tx_get_optional(&db, parent.as_str(), SESSIONS_SUBCOL, &session_id).await?;

        if let Some(session) = session_doc {
            if session.get("hasPlayed").and_then(|v| v.as_bool()) == Some(true) {
                if let Some(played_at) = session
                    .get("playedAt")
                    .and_then(|v| crate::utils::firestore::millis_from_value(v))
                {
                    let hours = (millis_now() - played_at) as f64 / (1000.0 * 60.0 * 60.0);
                    if hours < 12.0 {
                        return Err(tx_err("SESSION_COOLDOWN"));
                    }
                } else {
                    return Err(tx_err("SESSION_COOLDOWN"));
                }
            }
        }

        // Device + IP uniqueness hardening (hard block for one entry per person).
        // Primary key is the stable client-generated deviceId persisted in localStorage.
        if let Some(ref dev) = device_id {
            if let Some(existing) = UniquenessRepository::find_device_lock_tx(&db, parent.as_str(), dev)
                .await?
            {
                // Any prior lock for this device in the campaign means the device already participated.
                // We treat it as used (permanent for the campaign) to enforce "one entry".
                if existing.get("hasPlayed").and_then(|v| v.as_bool()) == Some(true)
                    || existing.get("sessionId").is_some()
                {
                    return Err(tx_err("DEVICE_ALREADY_USED"));
                }
            }
        }

        // (Future) IP + fingerprint correlation check could live here and return
        // tx_err("IP_DEVICE_CONFLICT") when policy decides a hard block is warranted.

        db.fluent()
            .update()
            .in_col(SESSIONS_SUBCOL)
            .document_id(&session_id)
            .parent(parent.as_str())
            .object(&json!({
                "fullName": full_name_upper,
                "sessionId": session_id,
                "hasPlayed": true,
                "playedAt": now,
                "deviceId": device_id,
            }))
            .add_to_transaction(transaction)
            .map_err(BackoffError::Permanent)?;

        db.fluent()
            .update()
            .in_col(REGISTRATIONS_SUBCOL)
            .document_id(&name_ref)
            .parent(parent.as_str())
            .object(&json!({
                "kind": "name_lock",
                "blocked": true,
                "fullName": full_name_upper,
                "normalizedName": normalized_name,
                "ip": ip,
                "userAgent": user_agent,
                "registeredAt": now,
                "deviceId": device_id,
            }))
            .add_to_transaction(transaction)
            .map_err(BackoffError::Permanent)?;

        // Atomically claim the device lock (primary anti-cheat signal).
        if let Some(ref dev) = device_id {
            let lock_id = UniquenessService::device_lock_id(dev);
            db.fluent()
                .update()
                .in_col(crate::features::uniqueness::domain::UNIQUENESS_SUBCOL)
                .document_id(&lock_id)
                .parent(parent.as_str())
                .object(&json!({
                    "kind": "device_lock",
                    "deviceId": dev,
                    "sessionId": session_id,
                    "hasPlayed": true,
                    "playedAt": now,
                    "registeredAt": now,
                    "ip": ip,
                    "userAgent": user_agent,
                    "deviceFingerprint": device_fingerprint,
                }))
                .add_to_transaction(transaction)
                .map_err(BackoffError::Permanent)?;
        }

        db.fluent()
            .update()
            .in_col(REGISTRATIONS_SUBCOL)
            .document_id(&session_id)
            .parent(parent.as_str())
            .object(&json!({
                "kind": "player",
                "sessionId": session_id,
                "fullName": full_name_upper,
                "normalizedName": normalized_name,
                "ip": ip,
                "userAgent": user_agent,
                "registeredAt": now,
                "locationId": location_id,
                "lat": lat,
                "lng": lng,
                "geoStatus": geo_status.as_str(),
                "ipLat": ip_lat,
                "ipLng": ip_lng,
                "ipGeoStatus": ip_geo_status,
                "deviceId": device_id,
                "deviceFingerprint": device_fingerprint,
            }))
            .add_to_transaction(transaction)
            .map_err(BackoffError::Permanent)?;

        Ok(())
    })
}

fn tx_err(code: &str) -> BackoffError<FirestoreError> {
    BackoffError::Permanent(FirestoreError::InvalidParametersError(
        FirestoreInvalidParametersError::new(FirestoreInvalidParametersPublicDetails::new(
            code.to_string(),
            "code".to_string(),
        )),
    ))
}

fn db_delete(
    db: &firestore::FirestoreDb,
    batch: &mut firestore::FirestoreBatch<'_, firestore::FirestoreSimpleBatchWriter>,
    parent: &str,
    collection: &str,
    id: &str,
) -> ApiResult<()> {
    db.fluent()
        .delete()
        .from(collection)
        .parent(parent)
        .document_id(id)
        .add_to_batch(batch)
        .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(())
}

fn map_registration_error(err: FirestoreError) -> ApiError {
    let msg = err.to_string();
    if msg.contains("NAME_TAKEN") {
        return ApiError::WithStatus {
            status: axum::http::StatusCode::CONFLICT,
            message: "The name has already been registered.".into(),
            code: Some("NAME_TAKEN".into()),
            data: None,
        };
    }
    if msg.contains("SESSION_COOLDOWN") {
        return ApiError::WithStatus {
            status: axum::http::StatusCode::CONFLICT,
            message: "You have already played. Please try again next time!".into(),
            code: Some("SESSION_COOLDOWN".into()),
            data: None,
        };
    }
    if msg.contains("DEVICE_ALREADY_USED") {
        return ApiError::WithStatus {
            status: axum::http::StatusCode::CONFLICT,
            message: "This device has already participated in the challenge. One entry per person.".into(),
            code: Some("DEVICE_ALREADY_USED".into()),
            data: None,
        };
    }
    if msg.contains("IP_DEVICE_CONFLICT") {
        return ApiError::WithStatus {
            status: axum::http::StatusCode::CONFLICT,
            message: "Device and location signals indicate this entry is a duplicate. One entry per person.".into(),
            code: Some("IP_DEVICE_CONFLICT".into()),
            data: None,
        };
    }
    ApiError::Internal(err.into())
}
