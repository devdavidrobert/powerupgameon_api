use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct VerifyBody {
    #[serde(rename = "idToken")]
    pub id_token: Option<String>,
}

pub async fn verify_token(
    State(state): State<Arc<AppState>>,
    Json(body): Json<VerifyBody>,
) -> ApiResult<Json<SuccessResponse<serde_json::Value>>> {
    let Some(id_token) = body.id_token.filter(|s| !s.is_empty()) else {
        return Err(ApiError::bad_request("idToken is required."));
    };

    let decoded = state.firebase_auth.verify_id_token(&id_token).await?;
    Ok(SuccessResponse::data(serde_json::json!({
        "uid": decoded.uid,
        "email": decoded.email,
        "emailVerified": decoded.email_verified,
    })))
}

pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(body): Json<VerifyBody>,
) -> ApiResult<impl IntoResponse> {
    let Some(id_token) = body.id_token.filter(|s| !s.is_empty()) else {
        return Err(ApiError::bad_request("idToken is required."));
    };

    let expires_in = 5 * 24 * 60 * 60 * 1000u64;
    let session_cookie = state
        .firebase_auth
        .create_session_cookie(&id_token, expires_in)
        .await?;

    let secure = if state.config.is_production { "; Secure" } else { "" };
    let cookie_header = format!(
        "__session={session_cookie}; HttpOnly; Path=/; SameSite=Strict; Max-Age={}{secure}",
        expires_in / 1000
    );

    Ok((
        StatusCode::OK,
        [(header::SET_COOKIE, HeaderValue::from_str(&cookie_header).unwrap())],
        Json(SuccessResponse::<serde_json::Value> {
            success: true,
            data: None,
            message: Some("Session created.".into()),
            code: None,
            next_cursor: None,
            has_more: None,
        }),
    ))
}
