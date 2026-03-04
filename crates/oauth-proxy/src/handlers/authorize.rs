use axum::{extract::{Query, State}, response::IntoResponse};
use claude_memory_common::{
    crypto::encrypt,
    dcr::verify_client_id,
    error::OAuthError,
    types::ProxyState,
    uri::{is_valid_redirect_uri, normalize_redirect_uri},
};
use serde::Deserialize;

use crate::AppState;
use crate::response::{oauth_error, redirect};

#[derive(Debug, Deserialize)]
pub struct AuthorizeParams {
    pub client_id: Option<String>,
    pub redirect_uri: Option<String>,
    pub state: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub response_type: Option<String>,
    pub scope: Option<String>,
}

pub async fn handle_authorize(
    State(state): State<AppState>,
    Query(params): Query<AuthorizeParams>,
) -> impl IntoResponse {
    let Some(client_id) = params.client_id.as_deref() else {
        return oauth_error(OAuthError::InvalidRequest(
            "client_id and redirect_uri are required".into(),
        ))
        .into_response();
    };

    let Some(redirect_uri) = params.redirect_uri.as_deref() else {
        return oauth_error(OAuthError::InvalidRequest(
            "client_id and redirect_uri are required".into(),
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

    let Some(client_info) = verify_client_id(client_id, server_secret) else {
        return oauth_error(OAuthError::InvalidClient("Invalid client_id".into())).into_response();
    };

    if !is_valid_redirect_uri(redirect_uri) {
        return oauth_error(OAuthError::InvalidRedirectUri(
            "Invalid redirect_uri".into(),
        ))
        .into_response();
    }

    // Validate redirect_uri against registered URIs (Open Redirect prevention)
    let normalized = normalize_redirect_uri(redirect_uri);
    let is_registered = client_info
        .redirect_uris
        .iter()
        .any(|registered| *registered == normalized);

    if !is_registered {
        return oauth_error(OAuthError::InvalidRedirectUri(
            "redirect_uri does not match registered URIs".into(),
        ))
        .into_response();
    }

    // Encrypt proxy state
    let proxy_state = encrypt(
        &serde_json::to_string(&ProxyState {
            redirect_uri: redirect_uri.to_string(),
            state: params.state,
            client_id: client_id.to_string(),
        })
        .unwrap(),
        server_secret,
    );

    // Build Cognito authorize URL
    let response_type = params.response_type.as_deref().unwrap_or("code");
    let scope = params.scope.as_deref().unwrap_or("openid email profile");

    let mut cognito_params = vec![
        ("response_type", response_type.to_string()),
        ("client_id", state.config.cognito_client_id.clone()),
        ("redirect_uri", format!("{}/callback", state.config.api_url)),
        ("scope", scope.to_string()),
        ("state", proxy_state),
    ];

    if let Some(code_challenge) = &params.code_challenge {
        cognito_params.push(("code_challenge", code_challenge.clone()));
        cognito_params.push((
            "code_challenge_method",
            params
                .code_challenge_method
                .as_deref()
                .unwrap_or("S256")
                .to_string(),
        ));
    }

    let query = serde_urlencoded::to_string(&cognito_params).unwrap();
    let location = format!(
        "https://{}/oauth2/authorize?{query}",
        state.config.cognito_domain
    );

    redirect(&location).into_response()
}
