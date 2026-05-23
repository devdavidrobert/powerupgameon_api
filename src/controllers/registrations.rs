use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::logger;
use crate::middleware::request_context::RequestContext;
use crate::models::registration::RegistrationModel;
use crate::models::submission::SubmissionModel;
use crate::utils::challenge_window::assert_challenge_open;
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
}

pub async fn get_all_registrations(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RegistrationQuery>,
) -> ApiResult<Json<SuccessResponse<Vec<Map<String, Value>>>>> {
    let limit = query.limit.unwrap_or(50);
    let cursor = query
        .cursor
        .as_deref()
        .and_then(decode_cursor)
        .and_then(|v| v.as_object().cloned());

    let (items, next_cursor, has_more) =
        RegistrationModel::find_player_page(&state, limit, cursor).await?;
    let ids: Vec<String> = items
        .iter()
        .filter_map(|r| r.get("id").or_else(|| r.get("sessionId")).and_then(|v| v.as_str()))
        .map(String::from)
        .collect();
    let completed = SubmissionModel::ids_that_exist(&state, &ids).await?;

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
    Extension(ctx): Extension<RequestContext>,
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

    assert_challenge_open(&state).await?;

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

    RegistrationModel::register(
        &state,
        session_id,
        &full_name,
        &normalized,
        &ip,
        &user_agent,
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
        }
        e
    })?;

    logger::log(
        &state.config,
        "info",
        "registration_ok",
        json!({ "requestId": ctx.request_id, "sessionId": session_id }),
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
    Path(id): Path<String>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    if RegistrationModel::find_by_id(&state, &id).await?.is_none() {
        return Err(ApiError::bad_request("Registration not found."));
    }
    RegistrationModel::delete(&state, &id).await?;
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
