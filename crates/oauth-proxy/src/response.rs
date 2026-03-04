use axum::{
    http::{StatusCode, header},
    response::IntoResponse,
};
use claude_memory_common::{error::OAuthError, types::OAuthErrorResponse};

pub fn json_response(status: u16, body: impl serde::Serialize) -> impl IntoResponse {
    (
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_string(&body).unwrap(),
    )
}

pub fn redirect(location: &str) -> impl IntoResponse {
    (StatusCode::FOUND, [(header::LOCATION, location.to_string())], String::new())
}

pub fn oauth_error(err: OAuthError) -> impl IntoResponse {
    json_response(
        err.status_code(),
        OAuthErrorResponse {
            error: err.error_code().to_string(),
            error_description: err.to_string(),
        },
    )
}
