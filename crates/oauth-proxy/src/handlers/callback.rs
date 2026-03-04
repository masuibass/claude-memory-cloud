use axum::{extract::{Query, State}, response::IntoResponse};
use claude_memory_common::{crypto::decrypt, error::OAuthError, types::ProxyState};
use serde::Deserialize;

use crate::AppState;
use crate::response::{oauth_error, redirect};

#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

pub async fn handle_callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> impl IntoResponse {
    let Some(proxy_state_enc) = params.state.as_deref() else {
        return oauth_error(OAuthError::InvalidRequest(
            "Missing state parameter".into(),
        ))
        .into_response();
    };

    let server_secret = match state.server_secret().await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to get server secret: {e}");
            return oauth_error(OAuthError::ServerError("Internal server error".into()))
                .into_response();
        }
    };

    let proxy_state: ProxyState = match decrypt(proxy_state_enc, server_secret)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
    {
        Some(s) => s,
        None => {
            return oauth_error(OAuthError::InvalidState(
                "Invalid or tampered state parameter".into(),
            ))
            .into_response();
        }
    };

    // Build redirect to the client's localhost
    let mut redirect_params = Vec::new();
    if let Some(code) = &params.code {
        redirect_params.push(("code", code.as_str()));
    }
    if let Some(original_state) = &proxy_state.state {
        redirect_params.push(("state", original_state.as_str()));
    }
    if let Some(error) = &params.error {
        redirect_params.push(("error", error.as_str()));
    }
    if let Some(desc) = &params.error_description {
        redirect_params.push(("error_description", desc.as_str()));
    }

    let query = serde_urlencoded::to_string(&redirect_params).unwrap();
    let separator = if proxy_state.redirect_uri.contains('?') {
        "&"
    } else {
        "?"
    };

    let location = format!("{}{separator}{query}", proxy_state.redirect_uri);
    redirect(&location).into_response()
}
