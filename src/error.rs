use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{message}")]
    WithStatus {
        status: StatusCode,
        message: String,
        code: Option<String>,
        data: Option<serde_json::Value>,
    },
    #[error("{0}")]
    Internal(#[from] anyhow::Error),
}

#[derive(Serialize)]
struct ErrorBody {
    success: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stack: Option<String>,
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::WithStatus {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
            code: None,
            data: None,
        }
    }

    pub fn with_code(
        status: StatusCode,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::WithStatus {
            status,
            message: message.into(),
            code: Some(code.into()),
            data: None,
        }
    }

    pub fn with_code_data(
        status: StatusCode,
        code: impl Into<String>,
        message: impl Into<String>,
        data: serde_json::Value,
    ) -> Self {
        Self::WithStatus {
            status,
            message: message.into(),
            code: Some(code.into()),
            data: Some(data),
        }
    }

    pub fn from_firestore_code(code: &str) -> Option<Self> {
        match code {
            "permission-denied" => Some(Self::WithStatus {
                status: StatusCode::FORBIDDEN,
                message: "Permission denied.".into(),
                code: None,
                data: None,
            }),
            "not-found" => Some(Self::WithStatus {
                status: StatusCode::NOT_FOUND,
                message: "Resource not found.".into(),
                code: None,
                data: None,
            }),
            "failed-precondition" => Some(Self::WithStatus {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: "A required Firestore composite index is missing. Check server logs for the direct Firebase Console index creation link.".into(),
                code: Some("FIRESTORE_INDEX_REQUIRED".into()),
                data: None,
            }),
            _ => None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let is_dev = std::env::var("NODE_ENV").unwrap_or_default() != "production";

        match self {
            ApiError::WithStatus {
                status,
                message,
                code,
                data,
            } => (
                status,
                Json(ErrorBody {
                    success: false,
                    message,
                    code,
                    data,
                    stack: None,
                }),
            )
                .into_response(),
            ApiError::Internal(err) => {
                tracing::error!(error = %err, "internal error");
                let message = err.to_string();
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorBody {
                        success: false,
                        message: if is_dev {
                            message
                        } else {
                            "An unexpected error occurred.".into()
                        },
                        code: None,
                        data: None,
                        stack: is_dev.then(|| format!("{err:?}")),
                    }),
                )
                    .into_response()
            }
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Serialize)]
pub struct SuccessResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(rename = "nextCursor", skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(rename = "hasMore", skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

impl<T: Serialize> SuccessResponse<T> {
    pub fn data(data: T) -> Json<Self> {
        Json(Self {
            success: true,
            data: Some(data),
            message: None,
            code: None,
            next_cursor: None,
            has_more: None,
        })
    }

    pub fn message(message: impl Into<String>) -> Json<Self> {
        Json(Self {
            success: true,
            data: None,
            message: Some(message.into()),
            code: None,
            next_cursor: None,
            has_more: None,
        })
    }
}

pub fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        Json(ErrorBody {
            success: false,
            message: message.into(),
            code: None,
            data: None,
            stack: None,
        }),
    )
        .into_response()
}

pub fn json_error_code(
    status: StatusCode,
    code: impl Into<String>,
    message: impl Into<String>,
) -> Response {
    (
        status,
        Json(ErrorBody {
            success: false,
            message: message.into(),
            code: Some(code.into()),
            data: None,
            stack: None,
        }),
    )
        .into_response()
}
