use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(Debug)]
pub enum AppError {
    Database(sqlx::Error),
    BadRequest(String),
    NotFound(String),
    Internal(String),
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::Database(err) => {
                tracing::error!(error = %err, "Database operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Erro interno ao processar dados no banco.".to_string(),
                )
                    .into_response()
            }
            AppError::BadRequest(msg) => {
                tracing::warn!(reason = %msg, "Bad request received");
                (StatusCode::BAD_REQUEST, msg).into_response()
            }
            AppError::NotFound(msg) => {
                tracing::warn!(reason = %msg, "Resource not found");
                (StatusCode::NOT_FOUND, msg).into_response()
            }
            AppError::Internal(msg) => {
                tracing::error!(error = %msg, "Internal application error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Erro interno do servidor.".to_string(),
                )
                    .into_response()
            }
        }
    }
}
