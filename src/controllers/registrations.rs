use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::features::campaigns::presentation::{CampaignContext, PublicCampaignContext};
use crate::logger;
use crate::middleware::request_context::RequestContext;
use crate::models::registration::{RegistrationInput, RegistrationModel};
use crate::models::submission::SubmissionModel;
use crate::utils::client_ip::get_client_ip;
use crate::utils::firestore::serialize_doc_data;
use crate::utils::helpers::{decode_cursor, encode_cursor, normalize_name};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct RegistrationQuery {
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}

#[derive(Deserialize)]
pub struct RegisterBody {
    #[serde(rename = "firstName")]
    pub first_name: Option<String>,
    #[serde(rename = "lastName")]
    pub last_name: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    #[serde(rename = "userAgent")]
    pub user_agent: Option<String>,
    #[serde(rename = "deviceId")]
    pub device_id: Option<String>,
    #[serde(rename = "deviceFingerprint")]
    pub device_fingerprint: Option<serde_json::Value>,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
}

pub async fn get_all_registrations(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Query(query): Query<RegistrationQuery>,
) -> ApiResult<Json<SuccessResponse<Vec<Map<String, Value>>>>> {
    let limit = query.limit.unwrap_or(50);
    let cursor = query
        .cursor
        .as_deref()
        .and_then(decode_cursor)
        .and_then(|v| v.as_object().cloned());

    let (items, next_cursor, has_more) =
        RegistrationModel::find_player_page(&state, &ctx.paths, limit, cursor).await?;
    let ids: Vec<String> = items
        .iter()
        .filter_map(|r| r.get("id").or_else(|| r.get("sessionId")).and_then(|v| v.as_str()))
        .map(String::from)
        .collect();
    let completed = SubmissionModel::ids_that_exist(&state, &ctx.paths, &ids).await?;

    let enriched: Vec<Map<String, Value>> = items
        .into_iter()
        .map(|reg| {
            let id = reg
                .get("id")
                .or_else(|| reg.get("sessionId"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let mut out = serialize_doc_data(&reg);
            out.insert("id".into(), json!(id.clone()));
            out.insert(
                "status".into(),
                json!(if completed.contains(&id) {
                    "completed"
                } else {
                    "incomplete"
                }),
            );
            out
        })
        .collect();

    Ok(Json(SuccessResponse {
        success: true,
        data: Some(enriched),
        message: None,
        code: None,
        next_cursor: next_cursor.map(|c| encode_cursor(&json!(c))),
        has_more: Some(has_more),
    }))
}

pub async fn register(
    State(state): State<Arc<AppState>>,
    PublicCampaignContext(ctx): PublicCampaignContext,
    Extension(req_ctx): Extension<RequestContext>,
    headers: axum::http::HeaderMap,
    Json(body): Json<RegisterBody>,
) -> ApiResult<(StatusCode, Json<SuccessResponse<Value>>)> {
    validate_name_part(body.first_name.as_deref(), "firstName")?;
    validate_name_part(body.last_name.as_deref(), "lastName")?;
    let session_id = body
        .session_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad_request("sessionId is required."))?;

    let lat = body
        .lat
        .ok_or_else(|| ApiError::bad_request("lat is required."))?;
    let lng = body
        .lng
        .ok_or_else(|| ApiError::bad_request("lng is required."))?;

    let first = body.first_name.as_ref().unwrap().trim();
    let last = body.last_name.as_ref().unwrap().trim();
    let full_name = format!("{first} {last}");
    let normalized = normalize_name(&full_name);
    let ip = get_client_ip(&headers, state.config.trust_proxy, "unknown");
    let user_agent = body
        .user_agent
        .as_deref()
        .or_else(|| headers.get("user-agent").and_then(|v| v.to_str().ok()))
        .unwrap_or("unknown")
        .to_string();

    let device_id = body.device_id.clone().filter(|s| !s.trim().is_empty());
    let device_fingerprint = body.device_fingerprint.clone();

    let geo_resolve = RegistrationModel::resolve_geo(
        &state,
        &ctx.paths,
        ctx.campaign.geo_enforcement,
        lat,
        lng,
        &ip,
    )
    .await?;

    RegistrationModel::register(
        &state,
        &ctx.paths,
        RegistrationInput {
            session_id: session_id.to_string(),
            full_name: full_name.clone(),
            normalized_name: normalized,
            ip,
            user_agent,
            lat,
            lng,
            location_id: geo_resolve.location_id,
            geo_status: geo_resolve.geo_status,
            ip_lat: geo_resolve.ip_lat,
            ip_lng: geo_resolve.ip_lng,
            ip_geo_status: geo_resolve.ip_geo_status,
            device_id,
            device_fingerprint,
        },
    )
    .await
    .map_err(|e| {
        if let ApiError::WithStatus { code: Some(code), .. } = &e {
            if code == "NAME_TAKEN" {
                return ApiError::with_code(
                    StatusCode::CONFLICT,
                    "NAME_TAKEN",
                    format!(
                        "The name \"{full_name}\" has already been registered. One entry per person."
                    ),
                );
            }
            if code == "DEVICE_ALREADY_USED" || code == "IP_DEVICE_CONFLICT" {
                // The model mapper already produced a good message + code; just ensure 409.
                return e;
            }
        }
        e
    })?;

    logger::log(
        &state.config,
        "info",
        "registration_ok",
        json!({
            "requestId": req_ctx.request_id,
            "sessionId": session_id,
            "campaignSlug": ctx.slug(),
        }),
    );

    Ok((
        StatusCode::CREATED,
        Json(SuccessResponse {
            success: true,
            data: Some(json!({
                "sessionId": session_id,
                "fullName": full_name.to_uppercase(),
            })),
            message: Some("Registration successful.".into()),
            code: None,
            next_cursor: None,
            has_more: None,
        }),
    ))
}

pub async fn delete_registration(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Path(id): Path<String>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    if RegistrationModel::find_by_id(&state, &ctx.paths, &id)
        .await?
        .is_none()
    {
        return Err(ApiError::bad_request("Registration not found."));
    }
    RegistrationModel::delete(&state, &ctx.paths, &id).await?;
    Ok(SuccessResponse::message(
        "Registration deleted. Player can now replay.",
    ))
}

fn validate_name_part(value: Option<&str>, label: &str) -> ApiResult<()> {
    let t = value.unwrap_or("").trim();
    if t.is_empty() {
        return Err(ApiError::bad_request(format!("{label} is required.")));
    }
    if t.len() > 50 {
        return Err(ApiError::bad_request(format!(
            "{label} must be at most 50 characters."
        )));
    }
    let mut chars = t.chars();
    let Some(first) = chars.next() else {
        return Err(ApiError::bad_request(format!("{label} is required.")));
    };
    if !first.is_ascii_alphanumeric() {
        return Err(ApiError::bad_request(format!(
            "{label} may only contain letters, numbers, spaces, apostrophes, and hyphens."
        )));
    }
    if !t.chars().all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '\'' || c == '-') {
        return Err(ApiError::bad_request(format!(
            "{label} may only contain letters, numbers, spaces, apostrophes, and hyphens."
        )));
    }
    Ok(())
}
