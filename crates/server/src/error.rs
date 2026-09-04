//! ApiError：实现 IntoResponse，映射 §4.5 统一错误体。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use folkmoot_common::errors::{ApiErrorBody, ErrorCode};

#[derive(Debug)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unauthorized, msg)
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::Forbidden, msg)
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, msg)
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, msg)
    }
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::ValidationError, msg)
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, msg)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if self.code == ErrorCode::Internal {
            tracing::error!(message = %self.message, "internal error");
        }
        let status = StatusCode::from_u16(self.code.http_status())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(ApiErrorBody::new(self.code, self.message))).into_response()
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        Self::internal(format!("sqlite: {e}"))
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        Self::internal(format!("{e:#}"))
    }
}
