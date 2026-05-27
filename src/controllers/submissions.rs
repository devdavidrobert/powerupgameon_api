use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::features::campaigns::presentation::{
    CampaignContext, PublicCampaignContext, SlugIdPath,
};
use crate::features::locations::domain::GeoStatus;
use crate::features::locations::infrastructure::LocationRepository;
use crate::features::spin::domain::{is_consolation_prize, prize_id_from_map};
use crate::middleware::request_context::RequestContext;
use crate::models::prize::PrizeModel;
use crate::models::registration::RegistrationModel;
use crate::models::submission::{SubmissionCreateInput, SubmissionModel};
use crate::utils::client_ip::{get_client_ip, ClientPeer};
use crate::utils::firestore::serialize_doc_data;
use crate::utils::helpers::{decode_cursor, encode_cursor, submission_identity_from_registration};
use crate::utils::spin_token::mint_spin_token;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};

#[derive(Deserialize)]
pub struct UpdateSubmissionPrizeGivenBody {
    #[serde(rename = "prizeGiven")]
    pub prize_given: Option<bool>,
}
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
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

    let locations = LocationRepository::find_all(&state, &ctx.paths).await?;
    let location_names: HashMap<String, String> = locations
        .iter()
        .map(|l| (l.id.clone(), l.name.clone()))
        .collect();
    let prizes = PrizeModel::find_all(&state, &ctx.paths).await?;
    let prize_catalog = build_prize_catalog(&prizes);

    let mut data: Vec<Map<String, Value>> = Vec::with_capacity(items.len());
    for row in items {
        let id = row
            .get("id")
            .or_else(|| row.get("sessionId"))
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| {
                row.get("_firestore_id")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            });

        if let Some(ref submission_id) = id {
            let _ = SubmissionModel::repair_list_index_fields(&state, &ctx.paths, submission_id, &row)
                .await;

            if row.get("fullName").is_none() {
                if let Ok(Some(reg)) =
                    RegistrationModel::find_by_id(&state, &ctx.paths, submission_id).await
                {
                    let mut enriched = row.clone();
                    if let Some(name) = reg.get("fullName") {
                        enriched.insert("fullName".into(), name.clone());
                    }
                    if let Some(geo) = reg.get("geoStatus") {
                        enriched.insert("geoStatus".into(), geo.clone());
                    }
                    if let Some(loc) = reg.get("locationId") {
                        enriched.insert("locationId".into(), loc.clone());
                    }
                    let mut out = serialize_doc_data(&enriched);
                    out.insert("id".into(), json!(submission_id));
                    enrich_submission_admin_fields(&mut out, &location_names, &prize_catalog);
                    data.push(out);
                    continue;
                }
            }
        }

        let id_value = id
            .map(|s| json!(s))
            .unwrap_or(Value::Null);
        let mut out = serialize_doc_data(&row);
        out.insert("id".into(), id_value);
        enrich_submission_admin_fields(&mut out, &location_names, &prize_catalog);
        data.push(out);
    }

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
    Path(path): Path<SlugIdPath>,
) -> ApiResult<Json<SuccessResponse<Map<String, Value>>>> {
    let sub = SubmissionModel::find_by_id(&state, &ctx.paths, &path.id)
        .await?
        .ok_or_else(|| ApiError::bad_request("Submission not found."))?;
    let mut out = serialize_doc_data(&sub);
    out.insert("id".into(), json!(path.id));
    Ok(SuccessResponse::data(out))
}

pub async fn create_submission(
    State(state): State<Arc<AppState>>,
    PublicCampaignContext(ctx): PublicCampaignContext,
    Extension(req_ctx): Extension<RequestContext>,
    ClientPeer(peer): ClientPeer,
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
        .ok_or_else(|| ApiError::bad_request("answers must be an array."))?;

    if answers_raw.is_empty() {
        return Err(ApiError::bad_request("answers cannot be empty."));
    }

    let sanitized: Vec<Value> = answers_raw;

    let registration = RegistrationModel::find_by_id(&state, &ctx.paths, session_id)
        .await?
        .ok_or_else(|| ApiError::bad_request("Registration not found for this session."))?;

    if let Some(existing) = SubmissionModel::find_by_id(&state, &ctx.paths, session_id).await? {
        return submission_success_response(&state.config, &ctx, session_id, existing, StatusCode::OK);
    }

    let (full_name, normalized) = submission_identity_from_registration(&registration)?;

    let location_id = registration
        .get("locationId")
        .and_then(|v| v.as_str())
        .map(String::from);
    let geo_status = registration
        .get("geoStatus")
        .and_then(|v| v.as_str())
        .map(GeoStatus::from_str)
        .unwrap_or(GeoStatus::NoZones);

    let ua = body
        .user_agent
        .as_deref()
        .filter(|s| s.len() < 2000)
        .or_else(|| headers.get("user-agent").and_then(|v| v.to_str().ok()))
        .unwrap_or("unknown")
        .to_string();
    let ip = get_client_ip(&headers, state.config.trust_proxy, peer);

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

    submission_success_response(
        &state.config,
        &ctx,
        session_id,
        result,
        StatusCode::CREATED,
    )
}

fn submission_success_response(
    config: &crate::config::Config,
    ctx: &CampaignContext,
    session_id: &str,
    result: Map<String, Value>,
    status: StatusCode,
) -> ApiResult<(StatusCode, Json<SuccessResponse<Value>>)> {
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
        match mint_spin_token(config, ctx.campaign_id(), session_id) {
            Ok(token) => {
                payload.insert("spinToken".into(), json!(token));
            }
            Err(err) => {
                return Err(err);
            }
        }
    }

    if status == StatusCode::OK {
        payload.insert("idempotent".into(), json!(true));
    }

    payload.remove("answers");
    payload.remove("ip");

    Ok((
        status,
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

pub async fn update_submission_prize_given(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Path(path): Path<SlugIdPath>,
    Json(body): Json<UpdateSubmissionPrizeGivenBody>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    let Some(prize_given) = body.prize_given else {
        return Err(ApiError::bad_request("prizeGiven must be a boolean."));
    };
    if SubmissionModel::find_by_id(&state, &ctx.paths, &path.id)
        .await?
        .is_none()
    {
        return Err(ApiError::bad_request("Submission not found."));
    }
    SubmissionModel::update_prize_given(&state, &ctx.paths, &path.id, prize_given).await?;
    Ok(SuccessResponse::message("Prize fulfillment status updated."))
}

pub async fn delete_submission(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Path(path): Path<SlugIdPath>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    if SubmissionModel::find_by_id(&state, &ctx.paths, &path.id)
        .await?
        .is_none()
    {
        return Err(ApiError::bad_request("Submission not found."));
    }
    SubmissionModel::delete(&state, &ctx.paths, &path.id).await?;
    Ok(SuccessResponse::message(
        "All player records deleted.",
    ))
}

#[derive(Clone)]
struct PrizeCatalogEntry {
    id: String,
    name: String,
    is_real_prize: bool,
}

struct PrizeCatalog {
    by_id: HashMap<String, PrizeCatalogEntry>,
    by_name: HashMap<String, PrizeCatalogEntry>,
}

fn build_prize_catalog(prizes: &[Map<String, Value>]) -> PrizeCatalog {
    let mut by_id = HashMap::new();
    let mut by_name = HashMap::new();
    for prize in prizes {
        let Some(id) = prize_id_from_map(prize) else {
            continue;
        };
        let name = prize
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        let entry = PrizeCatalogEntry {
            id: id.clone(),
            name: name.clone(),
            is_real_prize: !is_consolation_prize(prize),
        };
        by_id.insert(id, entry.clone());
        by_name.insert(name, entry);
    }
    PrizeCatalog { by_id, by_name }
}

fn enrich_submission_admin_fields(
    out: &mut Map<String, Value>,
    location_names: &HashMap<String, String>,
    catalog: &PrizeCatalog,
) {
    if let Some(location_id) = out.get("locationId").and_then(|v| v.as_str()) {
        if let Some(name) = location_names.get(location_id) {
            out.insert("locationName".into(), json!(name));
        }
    }

    let stored_prize = out
        .get("prize")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let stored_prize_id = out
        .get("prizeId")
        .and_then(|v| v.as_str())
        .map(String::from);

    let entry = stored_prize_id
        .as_ref()
        .and_then(|id| catalog.by_id.get(id))
        .or_else(|| {
            if stored_prize.is_empty() {
                None
            } else {
                catalog.by_name.get(&stored_prize)
            }
        });

    let Some(entry) = entry else {
        if out.get("isRealPrize").is_none()
            && !stored_prize.is_empty()
            && stored_prize != "pending"
            && stored_prize != "Nothing"
        {
            out.insert("isRealPrize".into(), json!(true));
        }
        return;
    };

    if stored_prize.is_empty() || stored_prize == "pending" {
        out.insert("prize".into(), json!(&entry.name));
    }
    if stored_prize_id.is_none() {
        out.insert("prizeId".into(), json!(&entry.id));
    }
    if out.get("isRealPrize").is_none() {
        out.insert("isRealPrize".into(), json!(entry.is_real_prize));
    }
}

fn map_create_error(err: ApiError, ctx: &RequestContext) -> ApiError {
    if let ApiError::WithStatus {
        code: Some(code),
        message,
        ..
    } = &err
    {
        if matches!(
            code.as_str(),
            "NO_SESSION" | "INVALID_ANSWERS_LENGTH" | "INVALID_ANSWER_INDEX" | "INVALID_ANSWER"
        ) {
            tracing::warn!(
                request_id = %ctx.request_id,
                code = %code,
                detail = %message,
                "submission_validation_failed"
            );
            if code == "INVALID_ANSWERS_LENGTH" {
                return ApiError::with_code(
                    StatusCode::CONFLICT,
                    "QUESTIONS_CHANGED",
                    "The quiz has changed since you started. Please refresh and try again.",
                );
            }
            return ApiError::bad_request(
                "Submission validation failed. Please refresh and try again.",
            );
        }
        if code == "NO_QUESTIONS" {
            return ApiError::WithStatus {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: "Game configuration error.".into(),
                code: None,
                data: None,
            };
        }
    }
    err
}
