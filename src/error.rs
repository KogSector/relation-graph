//! Error types for relation-graph service

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GraphError {
    #[error("Neo4j error: {0}")]
    Neo4j(String),
    
    #[error("Zilliz error: {0}")]
    Zilliz(String),
    
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("Entity not found: {0}")]
    EntityNotFound(String),
    
    #[error("Invalid entity type: {0}")]
    InvalidEntityType(String),
    
    #[error("Invalid relationship type: {0}")]
    InvalidRelationshipType(String),
    
    #[error("Embedding error: {0}")]
    Embedding(String),
    
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
    
    #[error("Internal error: {0}")]
    Internal(String),
}

pub type GraphResult<T> = Result<T, GraphError>;

impl IntoResponse for GraphError {
    fn into_response(self) -> Response {
        let (status, error_message) = match &self {
            GraphError::EntityNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            GraphError::InvalidEntityType(_) | GraphError::InvalidRelationshipType(_) => {
                (StatusCode::BAD_REQUEST, self.to_string())
            }
            GraphError::ServiceUnavailable(_) => {
                (StatusCode::SERVICE_UNAVAILABLE, self.to_string())
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };

        let body = Json(serde_json::json!({
            "error": error_message,
            "status": status.as_u16(),
        }));

        (status, body).into_response()
    }
}
