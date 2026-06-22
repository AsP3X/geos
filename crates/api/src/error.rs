//! Map [`geos_core::AppError`] to HTTP responses (`api-error-shape.mdc`).

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use geos_core::AppError;

/// Newtype wrapper so `IntoResponse` can be implemented in this crate.
#[derive(Debug)]
pub struct ApiError(pub AppError);

impl From<AppError> for ApiError {
    fn from(value: AppError) -> Self {
        Self(value)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let err = self.0;
        let status =
            StatusCode::from_u16(err.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(err.envelope())).into_response()
    }
}
