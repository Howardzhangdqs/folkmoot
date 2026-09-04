//! 统一错误体（§4.5）：`{ "error": { "code", "message", "details" } }`。

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthorized,
    Forbidden,
    CsrfRejected,
    NotFound,
    Conflict,
    PayloadTooLarge,
    UnsupportedMediaType,
    ValidationError,
    Internal,
}

impl ErrorCode {
    pub fn http_status(self) -> u16 {
        match self {
            Self::Unauthorized => 401,
            Self::Forbidden | Self::CsrfRejected => 403,
            Self::NotFound => 404,
            Self::Conflict => 409,
            Self::PayloadTooLarge => 413,
            Self::UnsupportedMediaType => 415,
            Self::ValidationError => 422,
            Self::Internal => 500,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiErrorBody {
    pub error: ApiErrorInner,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiErrorInner {
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ApiErrorBody {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            error: ApiErrorInner {
                code,
                message: message.into(),
                details: None,
            },
        }
    }
}
