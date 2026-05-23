use crate::app_state::AppState;
use crate::error::{json_error, json_error_code};
use crate::services::firebase_auth::VerifiedUser;
use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct AuthUser {
    pub uid: String,
    pub email: Option<String>,
    pub admin: bool,
}

impl From<VerifiedUser> for AuthUser {
    fn from(u: VerifiedUser) -> Self {
        Self {
            uid: u.uid,
            email: u.email,
            admin: u.admin,
        }
    }
}

pub fn user_has_admin_access(user: &AuthUser, allowed_admin_emails: &[String]) -> bool {
    if user.admin {
        return true;
    }
    let email = user.email.as_deref().unwrap_or("").to_lowercase();
    !allowed_admin_emails.is_empty()
        && !email.is_empty()
        && allowed_admin_emails.contains(&email)
}

pub fn parse_bearer_token(auth_header: Option<&str>) -> Option<&str> {
    auth_header?.strip_prefix("Bearer ")
}

pub async fn authenticate_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let Some(header) = auth_header else {
        return json_error(
            StatusCode::UNAUTHORIZED,
            "Unauthorized. A valid Bearer token is required.",
        );
    };

    let Some(token) = parse_bearer_token(Some(header)) else {
        return json_error(
            StatusCode::UNAUTHORIZED,
            "Unauthorized. A valid Bearer token is required.",
        );
    };

    match state.firebase_auth.verify_id_token(token).await {
        Ok(user) => {
            req.extensions_mut().insert(AuthUser::from(user));
            next.run(req).await
        }
        Err(_) => json_error(StatusCode::UNAUTHORIZED, "Invalid or expired token."),
    }
}

pub async fn require_admin_middleware(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let user = req.extensions().get::<AuthUser>().cloned();
    let Some(user) = user else {
        return json_error(
            StatusCode::UNAUTHORIZED,
            "Unauthorized. A valid Bearer token is required.",
        );
    };

    if user_has_admin_access(&user, &state.config.allowed_admin_emails) {
        return next.run(req).await;
    }

    json_error_code(
        StatusCode::FORBIDDEN,
        "FORBIDDEN_ADMIN",
        "Admin access required.",
    )
}
