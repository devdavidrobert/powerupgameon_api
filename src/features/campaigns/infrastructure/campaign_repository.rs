use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::domain::{
    BrandLogo, Campaign, CampaignStatus, GeoEnforcement, MAX_BRAND_LOGOS, PlayerOutcomeCopy,
    StaggerMode, StaggerStep,
};
use crate::features::campaigns::infrastructure::campaign_paths::CAMPAIGNS_COLLECTION;
use crate::utils::firestore::millis_now;
use firestore::FirestoreQueryDirection;
use serde_json::{json, Map, Value};
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
            "spinPassPercent": 100,
            "ipRateLimitWindowSecs": crate::features::campaigns::domain::DEFAULT_IP_RATE_LIMIT_WINDOW_SECS,
            "createdAt": now,
            "updatedAt": now,
        });

        state
            .db
            .client
            .fluent()
            .insert()
            .into(CAMPAIGNS_COLLECTION)
            .document_id(&id)
            .object(&payload)
            .execute::<Map<String, Value>>()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        Self::find_by_id(state, &id)
            .await?
            .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("Campaign create failed")))
    }

    pub async fn update(
        state: &AppState,
        id: &str,
        data: Map<String, Value>,
    ) -> ApiResult<Campaign> {
        let Some(mut payload): Option<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .by_id_in(CAMPAIGNS_COLLECTION)
            .obj()
            .one(id)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?
        else {
            return Err(ApiError::bad_request("Campaign not found."));
        };

        payload.retain(|key, _| !key.starts_with('_'));
        for (key, value) in data {
            payload.insert(key, value);
        }
        payload.insert("id".into(), json!(id));
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
    let status = CampaignStatus::from_str(
        doc.get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("draft"),
    );

    let stagger_schedule = doc
        .get("staggerSchedule")
        .and_then(|v| v.as_array())
        .map(|arr| {
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
            doc.get("staggerMode")
                .and_then(|v| v.as_str())
                .unwrap_or("linear"),
        ),
        stagger_schedule,
        geo_enforcement: GeoEnforcement::from_str(
            doc.get("geoEnforcement")
                .and_then(|v| v.as_str())
                .unwrap_or("reject"),
        ),
        spin_pass_percent: doc
            .get("spinPassPercent")
            .and_then(|v| v.as_i64())
            .unwrap_or(100)
            .clamp(0, 100),
        brand_logos: doc
            .get("brandLogos")
            .and_then(parse_brand_logos_from_value)
            .filter(|logos| !logos.is_empty()),
        player_outcome_copy: doc
            .get("playerOutcomeCopy")
            .and_then(parse_player_outcome_copy_from_value),
        registration_form_header: doc
            .get("registrationFormHeader")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        ip_rate_limit_window_secs: doc
            .get("ipRateLimitWindowSecs")
            .and_then(|v| v.as_i64()),
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
        "spinPassPercent": campaign.spin_pass_percent,
        "brandLogos": campaign.sorted_brand_logos(),
        "playerOutcomeCopy": campaign.player_outcome_copy_or_default(),
        "registrationFormHeader": campaign.registration_form_header_or_default(),
        "ipRateLimitWindowSecs": campaign.ip_rate_limit_window_secs(),
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
            return Err(ApiError::bad_request(
                "releasePercent must be between 0 and 1.",
            ));
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
    if body.clear_stagger_schedule {
        payload.insert("staggerSchedule".into(), Value::Null);
    } else if let Some(schedule) = &body.stagger_schedule {
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
    if let Some(spin_pass_percent) = body.spin_pass_percent {
        payload.insert(
            "spinPassPercent".into(),
            json!(spin_pass_percent.clamp(0, 100)),
        );
    }
    if body.clear_brand_logos {
        payload.insert("brandLogos".into(), Value::Null);
    } else if let Some(logos) = &body.brand_logos {
        payload.insert(
            "brandLogos".into(),
            json!(logos
                .iter()
                .map(|logo| json!({
                    "url": logo.url,
                    "alt": logo.alt,
                    "sortOrder": logo.sort_order,
                }))
                .collect::<Vec<_>>()),
        );
    }
    if body.clear_player_outcome_copy {
        payload.insert("playerOutcomeCopy".into(), Value::Null);
    } else if let Some(copy) = &body.player_outcome_copy {
        payload.insert(
            "playerOutcomeCopy".into(),
            serde_json::to_value(copy).map_err(|e| ApiError::Internal(e.into()))?,
        );
    }
    if body.clear_registration_form_header {
        payload.insert("registrationFormHeader".into(), Value::Null);
    } else     if let Some(header) = &body.registration_form_header {
        payload.insert("registrationFormHeader".into(), json!(header));
    }
    if let Some(window_secs) = body.ip_rate_limit_window_secs {
        payload.insert(
            "ipRateLimitWindowSecs".into(),
            json!(window_secs),
        );
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
    pub clear_stagger_schedule: bool,
    pub geo_enforcement: Option<GeoEnforcement>,
    pub spin_pass_percent: Option<i64>,
    pub brand_logos: Option<Vec<BrandLogo>>,
    pub clear_brand_logos: bool,
    pub player_outcome_copy: Option<PlayerOutcomeCopy>,
    pub clear_player_outcome_copy: bool,
    pub registration_form_header: Option<String>,
    pub clear_registration_form_header: bool,
    pub ip_rate_limit_window_secs: Option<i64>,
}

pub fn parse_challenge_time_value(value: &Value) -> ApiResult<Value> {
    match value {
        Value::Null => Ok(Value::Null),
        Value::Number(n) => {
            if let Some(ms) = n.as_i64() {
                return Ok(json!(ms));
            }
            if let Some(ms) = n.as_u64() {
                return Ok(json!(ms as i64));
            }
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f >= 0.0 {
                    return Ok(json!(f.round() as i64));
                }
            }
            Err(ApiError::bad_request("Invalid challenge time."))
        }
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return Ok(Value::Null);
            }
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(trimmed) {
                return Ok(json!(dt.timestamp_millis()));
            }
            for fmt in [
                "%Y-%m-%dT%H:%M:%S%.f",
                "%Y-%m-%dT%H:%M:%S",
                "%Y-%m-%dT%H:%M",
            ] {
                if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(trimmed, fmt) {
                    return Ok(json!(naive.and_utc().timestamp_millis()));
                }
            }
            Err(ApiError::bad_request("Invalid challenge time."))
        }
        _ => Err(ApiError::bad_request("Invalid challenge time.")),
    }
}

pub fn validate_challenge_window(payload: &Map<String, Value>) -> ApiResult<()> {
    if let (Some(start), Some(end)) = (
        payload.get("challengeStartTime").and_then(|v| v.as_i64()),
        payload.get("challengeEndTime").and_then(|v| v.as_i64()),
    ) {
        if start >= end {
            return Err(ApiError::bad_request(
                "challengeEndTime must be after challengeStartTime.",
            ));
        }
    }
    Ok(())
}

pub fn parse_brand_logos_from_value(value: &Value) -> Option<Vec<BrandLogo>> {
    value.as_array().map(|arr| {
        arr.iter()
            .filter_map(|item| {
                Some(BrandLogo {
                    url: item.get("url")?.as_str()?.trim().to_string(),
                    alt: item
                        .get("alt")
                        .and_then(|v| v.as_str())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty()),
                    sort_order: item.get("sortOrder").and_then(|v| v.as_i64()).unwrap_or(0),
                })
            })
            .filter(|logo| !logo.url.is_empty())
            .collect()
    })
}

pub fn parse_brand_logos(value: &Value) -> ApiResult<Vec<BrandLogo>> {
    let Some(arr) = value.as_array() else {
        return Err(ApiError::bad_request("brandLogos must be an array."));
    };
    if arr.is_empty() {
        return Ok(vec![]);
    }
    if arr.len() > MAX_BRAND_LOGOS {
        return Err(ApiError::bad_request(format!(
            "brandLogos may contain at most {MAX_BRAND_LOGOS} logos."
        )));
    }

    let mut logos = Vec::new();
    for (index, item) in arr.iter().enumerate() {
        let url = item
            .get("url")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ApiError::bad_request("Each brand logo needs a url."))?;
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(ApiError::bad_request(
                "Brand logo url must start with http:// or https://.",
            ));
        }
        let alt = item
            .get("alt")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let sort_order = item
            .get("sortOrder")
            .and_then(|v| v.as_i64())
            .unwrap_or(index as i64);
        logos.push(BrandLogo {
            url: url.to_string(),
            alt,
            sort_order,
        });
    }

    logos.sort_by_key(|logo| logo.sort_order);
    Ok(logos)
}

pub fn parse_registration_form_header(value: &str) -> ApiResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad_request(
            "registrationFormHeader cannot be empty.",
        ));
    }
    if trimmed.chars().count() > crate::features::campaigns::domain::MAX_REGISTRATION_FORM_HEADER_LEN {
        return Err(ApiError::bad_request(format!(
            "registrationFormHeader must be at most {} characters.",
            crate::features::campaigns::domain::MAX_REGISTRATION_FORM_HEADER_LEN
        )));
    }
    Ok(trimmed.to_string())
}

pub fn parse_ip_rate_limit_window_secs(value: i64) -> ApiResult<i64> {
    use crate::features::campaigns::domain::{
        MAX_IP_RATE_LIMIT_WINDOW_SECS, MIN_IP_RATE_LIMIT_WINDOW_SECS,
    };
    if !(MIN_IP_RATE_LIMIT_WINDOW_SECS..=MAX_IP_RATE_LIMIT_WINDOW_SECS).contains(&value) {
        return Err(ApiError::bad_request(format!(
            "ipRateLimitWindowSecs must be between {MIN_IP_RATE_LIMIT_WINDOW_SECS} and {MAX_IP_RATE_LIMIT_WINDOW_SECS}."
        )));
    }
    Ok(value)
}

pub fn parse_player_outcome_copy_from_value(value: &Value) -> Option<PlayerOutcomeCopy> {
    parse_player_outcome_copy(value).ok()
}

pub fn parse_player_outcome_copy(value: &Value) -> ApiResult<PlayerOutcomeCopy> {
    use crate::features::campaigns::domain::{
        trim_optional, MAX_EXIT_BUTTON_LABEL_LEN, MAX_OUTCOME_FIELD_LEN, MAX_OUTCOME_TITLE_LEN,
    };

    if value.is_null() {
        return Ok(PlayerOutcomeCopy::default());
    }

    let Some(obj) = value.as_object() else {
        return Err(ApiError::bad_request("playerOutcomeCopy must be an object."));
    };

    let copy = PlayerOutcomeCopy {
        win_title: trim_optional(
            obj.get("winTitle")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            MAX_OUTCOME_TITLE_LEN,
        ),
        win_message: trim_optional(
            obj.get("winMessage")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            MAX_OUTCOME_FIELD_LEN,
        ),
        consolation_title: trim_optional(
            obj.get("consolationTitle")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            MAX_OUTCOME_TITLE_LEN,
        ),
        consolation_message: trim_optional(
            obj.get("consolationMessage")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            MAX_OUTCOME_FIELD_LEN,
        ),
        below_threshold_title: trim_optional(
            obj.get("belowThresholdTitle")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            MAX_OUTCOME_TITLE_LEN,
        ),
        below_threshold_message: trim_optional(
            obj.get("belowThresholdMessage")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            MAX_OUTCOME_FIELD_LEN,
        ),
        exit_button_label: trim_optional(
            obj.get("exitButtonLabel")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            MAX_EXIT_BUTTON_LABEL_LEN,
        ),
        exit_button_url: trim_optional(
            obj.get("exitButtonUrl")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            MAX_OUTCOME_FIELD_LEN,
        ),
    };

    if let Some(url) = &copy.exit_button_url {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(ApiError::bad_request(
                "exitButtonUrl must start with http:// or https://.",
            ));
        }
    }

    Ok(copy)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_update_payload_clears_stagger_schedule_when_requested() {
        let payload = build_update_payload(&CampaignUpdateInput {
            stagger_mode: Some(StaggerMode::Linear),
            clear_stagger_schedule: true,
            ..Default::default()
        })
        .unwrap();

        assert_eq!(payload.get("staggerMode").and_then(|v| v.as_str()), Some("linear"));
        assert_eq!(payload.get("staggerSchedule"), Some(&Value::Null));
    }

    #[test]
    fn build_update_payload_writes_stagger_steps_when_present() {
        let payload = build_update_payload(&CampaignUpdateInput {
            stagger_mode: Some(StaggerMode::Stepped),
            stagger_schedule: Some(vec![StaggerStep {
                release_at: 1_700_000_000_000,
                release_percent: 0.5,
            }]),
            ..Default::default()
        })
        .unwrap();

        assert!(payload.get("staggerSchedule").and_then(|v| v.as_array()).is_some());
    }

    #[test]
    fn parse_registration_form_header_rejects_blank() {
        assert!(parse_registration_form_header("   ").is_err());
    }

    #[test]
    fn parse_registration_form_header_trims_value() {
        assert_eq!(
            parse_registration_form_header("  Rider Details  ").unwrap(),
            "Rider Details"
        );
    }

    #[test]
    fn parse_brand_logos_rejects_more_than_two() {
        let value = json!([
            { "url": "https://a.example/logo.png", "sortOrder": 0 },
            { "url": "https://b.example/logo.png", "sortOrder": 1 },
            { "url": "https://c.example/logo.png", "sortOrder": 2 }
        ]);
        assert!(parse_brand_logos(&value).is_err());
    }

    #[test]
    fn parse_brand_logos_sorts_by_sort_order() {
        let value = json!([
            { "url": "https://b.example/logo.png", "sortOrder": 1, "alt": "B" },
            { "url": "https://a.example/logo.png", "sortOrder": 0, "alt": "A" }
        ]);
        let logos = parse_brand_logos(&value).unwrap();
        assert_eq!(logos.len(), 2);
        assert_eq!(logos[0].url, "https://a.example/logo.png");
        assert_eq!(logos[1].url, "https://b.example/logo.png");
    }
}
