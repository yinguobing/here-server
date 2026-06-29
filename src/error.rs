use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tracing::error;

// ---------------------------------------------------------------------------
// Shared error type for both public and admin APIs
// ---------------------------------------------------------------------------

pub enum AppError {
    Unauthorized,
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized"),
            AppError::Internal(msg) => {
                error!("{msg}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
            }
        }
        .into_response()
    }
}
