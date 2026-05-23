use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::domain::{
    Campaign, CampaignStatus, GeoEnforcement, StaggerMode, StaggerStep,
};
use crate::features::campaigns::infrastructure::campaign_paths::CAMPAIGNS_COLLECTION;
use crate::utils::firestore::millis_now;
use firestore::FirestoreQueryDirection;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

pub struct CampaignRepository;

impl CampaignRepository {
    pub async fn find_all(state: &AppState) -> ApiResult<Vec<Campaign>> {
        let rows: Vec<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .from(CAMPAIGNS_COLLECTION)
            .order_by([("createdAt", FirestoreQueryDirection::Descending)])
            .obj()
            .query()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(rows.into_iter().filter_map(map_campaign).collect())
    }

    pub async fn find_by_slug(state: &AppState, slug: &str) -> ApiResult<Option<Campaign>> {
        let rows: Vec<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .from(CAMPAIGNS_COLLECTION)
            .filter(|q| q.field("slug").eq(slug))
            .obj()
            .query()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(rows.into_iter().next().and_then(map_campaign))
    }

    pub async fn find_by_id(state: &AppState, id: &str) -> ApiResult<Option<Campaign>> {
        let doc: Option<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .by_id_in(CAMPAIGNS_COLLECTION)
            .obj()
            .one(id)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Ok(doc.and_then(map_campaign))
    }

    pub async fn slug_exists(state: &AppState, slug: &str) -> ApiResult<bool> {
        Ok(Self::find_by_slug(state, slug).await?.is_some())
    }

    pub async fn create(
        state: &AppState,
        slug: &str,
        name: &str,
        status: CampaignStatus,
    ) -> ApiResult<Campaign> {
        if Self::slug_exists(state, slug).await? {
            return Err(ApiError::with_code(
                axum::http::StatusCode::CONFLICT,
                "SLUG_TAKEN",
                "A campaign with this slug already exists.",
            ));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = millis_now();
        let payload = json!({
            "id": id,
            "slug": slug,
            "name": name,
            "status": status.as_str(),
            "challengeStartTime": null,
            "challengeEndTime": null,
            "staggerMode": StaggerMode::Linear.as_str(),
            "staggerSchedule": null,
            "geoEnforcement": GeoEnforcement::Reject.as_str(),
            "createdAt": now,
            "updatedAt": now,
        });

        state
            .db
            .client
            .fluent()
            .update()
            .in_col(CAMPAIGNS_COLLECTION)
            .document_id(&id)
            .object(&payload)
            .execute::<Map<String, Value>>()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Self::find_by_id(state, &id)
            .await?
            .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("Campaign create failed")))
    }

    pub async fn update(state: &AppState, id: &str, data: Map<String, Value>) -> ApiResult<Campaign> {
        let mut payload = data;
        payload.insert("updatedAt".into(), json!(millis_now()));

        state
            .db
            .client
            .fluent()
            .update()
            .in_col(CAMPAIGNS_COLLECTION)
            .document_id(id)
            .object(&payload)
            .execute::<Map<String, Value>>()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Self::find_by_id(state, id)
            .await?
            .ok_or_else(|| ApiError::bad_request("Campaign not found."))
    }

    pub async fn archive(state: &AppState, id: &str) -> ApiResult<()> {
        Self::update(
            state,
            id,
            Map::from_iter([("status".into(), json!(CampaignStatus::Archived.as_str()))]),
        )
        .await?;
        Ok(())
    }
}

pub fn map_campaign(doc: Map<String, Value>) -> Option<Campaign> {
    let id = doc.get("id").and_then(|v| v.as_str())?.to_string();

    let slug = doc.get("slug")?.as_str()?.to_string();
    let name = doc.get("name")?.as_str()?.to_string();
    let status = CampaignStatus::from_str(doc.get("status").and_then(|v| v.as_str()).unwrap_or("draft"));

    let stagger_schedule = doc.get("staggerSchedule").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|step| {
                Some(StaggerStep {
                    release_at: step.get("releaseAt")?.as_i64()?,
                    release_percent: step.get("releasePercent")?.as_f64()?,
                })
            })
            .collect()
    });

    Some(Campaign {
        id,
        slug,
        name,
        status,
        challenge_start_time: doc
            .get("challengeStartTime")
            .and_then(|v| crate::utils::firestore::millis_from_value(v)),
        challenge_end_time: doc
            .get("challengeEndTime")
            .and_then(|v| crate::utils::firestore::millis_from_value(v)),
        stagger_mode: StaggerMode::from_str(
            doc.get("staggerMode").and_then(|v| v.as_str()).unwrap_or("linear"),
        ),
        stagger_schedule,
        geo_enforcement: GeoEnforcement::from_str(
            doc.get("geoEnforcement").and_then(|v| v.as_str()).unwrap_or("reject"),
        ),
        created_at: doc
            .get("createdAt")
            .and_then(|v| crate::utils::firestore::millis_from_value(v)),
        updated_at: doc
            .get("updatedAt")
            .and_then(|v| crate::utils::firestore::millis_from_value(v)),
    })
}

pub fn campaign_to_json(campaign: &Campaign) -> Value {
    json!({
        "id": campaign.id,
        "slug": campaign.slug,
        "name": campaign.name,
        "status": campaign.status.as_str(),
        "challengeStartTime": campaign.challenge_start_time,
        "challengeEndTime": campaign.challenge_end_time,
        "staggerMode": campaign.stagger_mode.as_str(),
        "staggerSchedule": campaign.stagger_schedule,
        "geoEnforcement": campaign.geo_enforcement.as_str(),
        "createdAt": campaign.created_at,
        "updatedAt": campaign.updated_at,
    })
}

pub fn parse_stagger_schedule(value: &Value) -> ApiResult<Vec<StaggerStep>> {
    let Some(arr) = value.as_array() else {
        return Err(ApiError::bad_request("staggerSchedule must be an array."));
    };
    let mut steps = Vec::new();
    for step in arr {
        let release_at = step
            .get("releaseAt")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ApiError::bad_request("Each stagger step needs releaseAt."))?;
        let release_percent = step
            .get("releasePercent")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| ApiError::bad_request("Each stagger step needs releasePercent."))?;
        if !(0.0..=1.0).contains(&release_percent) {
            return Err(ApiError::bad_request("releasePercent must be between 0 and 1."));
        }
        steps.push(StaggerStep {
            release_at,
            release_percent,
        });
    }
    steps.sort_by_key(|s| s.release_at);
    Ok(steps)
}

pub fn build_update_payload(body: &CampaignUpdateInput) -> ApiResult<Map<String, Value>> {
    let mut payload = Map::new();
    if let Some(name) = &body.name {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(ApiError::bad_request("name cannot be empty."));
        }
        payload.insert("name".into(), json!(trimmed));
    }
    if let Some(status) = &body.status {
        payload.insert("status".into(), json!(status.as_str()));
    }
    if let Some(start) = &body.challenge_start_time {
        payload.insert("challengeStartTime".into(), start.clone());
    }
    if let Some(end) = &body.challenge_end_time {
        payload.insert("challengeEndTime".into(), end.clone());
    }
    if let Some(mode) = &body.stagger_mode {
        payload.insert("staggerMode".into(), json!(mode.as_str()));
    }
    if let Some(schedule) = &body.stagger_schedule {
        payload.insert(
            "staggerSchedule".into(),
            json!(schedule
                .iter()
                .map(|s| json!({
                    "releaseAt": s.release_at,
                    "releasePercent": s.release_percent,
                }))
                .collect::<Vec<_>>()),
        );
    }
    if let Some(geo) = &body.geo_enforcement {
        payload.insert("geoEnforcement".into(), json!(geo.as_str()));
    }
    Ok(payload)
}

#[derive(Debug, Default)]
pub struct CampaignUpdateInput {
    pub name: Option<String>,
    pub status: Option<CampaignStatus>,
    pub challenge_start_time: Option<Value>,
    pub challenge_end_time: Option<Value>,
    pub stagger_mode: Option<StaggerMode>,
    pub stagger_schedule: Option<Vec<StaggerStep>>,
    pub geo_enforcement: Option<GeoEnforcement>,
}

pub fn validate_slug(slug: &str) -> ApiResult<()> {
    if slug.is_empty() || slug.len() > 64 {
        return Err(ApiError::bad_request("slug must be 1-64 characters."));
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(ApiError::bad_request(
            "slug may only contain lowercase letters, digits, and hyphens.",
        ));
    }
    Ok(())
}

#[allow(dead_code)]
pub fn slug_index(rows: &[Campaign]) -> HashMap<String, String> {
    rows.iter()
        .map(|c| (c.slug.clone(), c.id.clone()))
        .collect()
}
