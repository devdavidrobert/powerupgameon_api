use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::features::campaigns::presentation::{CampaignContext, PublicCampaignContext};
use crate::logger;
use crate::middleware::request_context::RequestContext;
use crate::models::registration::RegistrationModel;
use crate::models::submission::{SubmissionCreateInput, SubmissionModel};
use crate::utils::challenge_window::assert_challenge_open_for_campaign;
use crate::utils::client_ip::get_client_ip;
use crate::utils::firestore::serialize_doc_data;
use crate::utils::helpers::{decode_cursor, encode_cursor, submission_identity_from_registration};
use crate::utils::spin_token::mint_spin_token;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct SubmissionQuery {
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateSubmissionBody {
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    #[serde(rename = "fullName")]
    pub full_name: Option<String>,
    #[serde(rename = "normalizedName")]
    pub normalized_name: Option<String>,
    pub answers: Option<Vec<serde_json::Value>>,
    #[serde(rename = "userAgent")]
    pub user_agent: Option<String>,
}

pub async fn get_all_submissions(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Query(query): Query<SubmissionQuery>,
) -> ApiResult<Json<SuccessResponse<Vec<Map<String, Value>>>>> {
    let limit = query.limit.unwrap_or(50);
    let cursor = query
        .cursor
        .as_deref()
        .and_then(decode_cursor)
        .and_then(|v| v.as_object().cloned());

    let (items, next_cursor, has_more) =
        SubmissionModel::find_page(&state, &ctx.paths, limit, cursor).await?;

    let data: Vec<Map<String, Value>> = items
        .into_iter()
        .map(|row| {
            let id = row
                .get("id")
                .or_else(|| row.get("sessionId"))
                .cloned()
                .unwrap_or(Value::Null);
            let mut out = serialize_doc_data(&row);
            out.insert("id".into(), id);
            out
        })
        .collect();

    Ok(Json(SuccessResponse {
        success: true,
        data: Some(data),
        message: None,
        code: None,
        next_cursor: next_cursor.map(|c| encode_cursor(&json!(c))),
        has_more: Some(has_more),
    }))
}

pub async fn get_submission(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Path(id): Path<String>,
) -> ApiResult<Json<SuccessResponse<Map<String, Value>>>> {
    let sub = SubmissionModel::find_by_id(&state, &ctx.paths, &id)
        .await?
        .ok_or_else(|| ApiError::bad_request("Submission not found."))?;
    let mut out = serialize_doc_data(&sub);
    out.insert("id".into(), json!(id));
    Ok(SuccessResponse::data(out))
}

pub async fn create_submission(
    State(state): State<Arc<AppState>>,
    PublicCampaignContext(ctx): PublicCampaignContext,
    Extension(req_ctx): Extension<RequestContext>,
    headers: axum::http::HeaderMap,
    Json(body): Json<CreateSubmissionBody>,
) -> ApiResult<(StatusCode, Json<SuccessResponse<Value>>)> {
    let session_id = body
        .session_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad_request("sessionId is required."))?;
    let answers_raw = body
        .answers
        .ok_or_else(|| ApiError::bad_request("answers must be an array of option indices."))?;

    let mut sanitized = Vec::new();
    for raw in answers_raw {
        let n = match raw {
            Value::Number(num) => num.as_i64(),
            Value::String(s) => s.parse().ok(),
            _ => None,
        };
        let Some(n) = n else {
            return Err(ApiError::bad_request("One or more answers are invalid."));
        };
        sanitized.push(n);
    }

    assert_challenge_open_for_campaign(&ctx.campaign)?;

    let registration = RegistrationModel::find_by_id(&state, &ctx.paths, session_id)
        .await?
        .ok_or_else(|| ApiError::bad_request("Registration not found for this session."))?;

    let (full_name, normalized) = submission_identity_from_registration(&registration)?;

    let location_id = registration
        .get("locationId")
        .and_then(|v| v.as_str())
        .map(String::from);
    let geo_status = registration
        .get("geoStatus")
        .and_then(|v| v.as_str())
        .unwrap_or("no_zones")
        .to_string();

    let ua = body
        .user_agent
        .as_deref()
        .filter(|s| s.len() < 2000)
        .or_else(|| headers.get("user-agent").and_then(|v| v.to_str().ok()))
        .unwrap_or("unknown")
        .to_string();
    let ip = get_client_ip(&headers, state.config.trust_proxy, "unknown");

    let result = SubmissionModel::create(
        &state,
        &ctx.paths,
        SubmissionCreateInput {
            session_id: session_id.to_string(),
            full_name: full_name.to_string(),
            normalized_name: normalized,
            answers: sanitized,
            user_agent: ua,
            ip,
            location_id,
            geo_status,
        },
    )
    .await
    .map_err(|e| map_create_error(e, &req_ctx))?;

    let mut payload = serialize_doc_data(&result);
    payload.insert(
        "id".into(),
        result
            .get("id")
            .or_else(|| result.get("sessionId"))
            .cloned()
            .unwrap_or(json!(session_id)),
    );

    if result.get("prize").and_then(|v| v.as_str()) == Some("pending")
        && result.get("status").and_then(|v| v.as_str()) == Some("pending")
    {
        match mint_spin_token(&state.config, ctx.campaign_id(), session_id) {
            Ok(token) => {
                payload.insert("spinToken".into(), json!(token));
            }
            Err(err) => {
                logger::log(
                    &state.config,
                    "error",
                    "spin_token_mint_failed",
                    json!({ "requestId": req_ctx.request_id, "err": err.to_string() }),
                );
                return Err(err);
            }
        }
    }

    payload.remove("answers");
    payload.remove("ip");

    Ok((
        StatusCode::CREATED,
        Json(SuccessResponse {
            success: true,
            data: Some(Value::Object(payload)),
            message: None,
            code: None,
            next_cursor: None,
            has_more: None,
        }),
    ))
}

pub async fn delete_submission(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Path(id): Path<String>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    if SubmissionModel::find_by_id(&state, &ctx.paths, &id)
        .await?
        .is_none()
    {
        return Err(ApiError::bad_request("Submission not found."));
    }
    SubmissionModel::delete(&state, &ctx.paths, &id).await?;
    Ok(SuccessResponse::message(
        "Submission and linked registration records deleted.",
    ))
}

fn map_create_error(err: ApiError, ctx: &RequestContext) -> ApiError {
    if let ApiError::WithStatus { code: Some(code), message, .. } = &err {
        if matches!(
            code.as_str(),
            "NO_SESSION" | "INVALID_ANSWERS_LENGTH" | "INVALID_ANSWER_INDEX"
        ) {
            tracing::warn!(
                request_id = %ctx.request_id,
                code = %code,
                detail = %message,
                "submission_validation_failed"
            );
            return ApiError::bad_request(
                "Submission validation failed. Please refresh and try again.",
            );
        }
        if code == "NO_QUESTIONS" {
            return ApiError::WithStatus {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: "Game configuration error.".into(),
                code: None,
            };
        }
    }
    err
}
