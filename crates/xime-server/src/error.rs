use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Invalid token")]
    InvalidToken,

    #[error("Token expired")]
    TokenExpired,

    #[error("Device not paired")]
    DeviceNotPaired,

    #[error("Pairing code not found")]
    PairCodeNotFound,

    #[error("Pairing code expired")]
    PairCodeExpired,

    #[error("Pairing already confirmed")]
    PairAlreadyConfirmed,

    #[error("Clipboard read failed: {0}")]
    ClipboardReadFailed(String),

    #[error("Clipboard write failed: {0}")]
    ClipboardWriteFailed(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Hash mismatch")]
    HashMismatch,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::InvalidToken | ApiError::TokenExpired | ApiError::DeviceNotPaired => {
                (StatusCode::UNAUTHORIZED, self.to_string())
            }
            ApiError::PairCodeNotFound | ApiError::PairCodeExpired => {
                (StatusCode::NOT_FOUND, self.to_string())
            }
            ApiError::PairAlreadyConfirmed => (StatusCode::CONFLICT, self.to_string()),
            ApiError::InvalidRequest(_) | ApiError::HashMismatch => {
                (StatusCode::BAD_REQUEST, self.to_string())
            }
            ApiError::ClipboardReadFailed(_) | ApiError::ClipboardWriteFailed(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
            }
            ApiError::InternalError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };

        let body = Json(crate::types::ErrorResponse {
            error: message,
            code: status.as_u16(),
        });

        (status, body).into_response()
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        ApiError::InternalError(e.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        ApiError::InternalError(e.to_string())
    }
}
