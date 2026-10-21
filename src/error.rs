//! The application error type and its mapping to an HTTP response.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

    #[error("{0}")]
    Validation(String),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }
}

impl From<sea_orm::DbErr> for AppError {
    fn from(err: sea_orm::DbErr) -> Self {
        Self::Internal(anyhow::Error::new(err))
    }
}

impl From<sea_orm::TransactionError<AppError>> for AppError {
    fn from(error: sea_orm::TransactionError<AppError>) -> Self {
        match error {
            sea_orm::TransactionError::Connection(error) => error.into(),
            sea_orm::TransactionError::Transaction(error) => error,
        }
    }
}

#[derive(serde::Serialize)]
struct ErrorResponse<'a> {
    error: &'a str,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not found"),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::Validation(msg) => (StatusCode::BAD_REQUEST, msg.as_str()),
            AppError::Internal(_) => {
                // Database errors can contain row values, including learner PII.
                tracing::error!("internal request error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
            }
        };
        (status, Json(ErrorResponse { error: message })).into_response()
    }
}
