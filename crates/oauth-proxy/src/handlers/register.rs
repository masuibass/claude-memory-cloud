use axum::{Json, extract::State, response::IntoResponse};
use claude_memory_common::{
    dcr::generate_client_credentials,
    error::OAuthError,
    types::{RegisterRequest, RegisterResponse},
    uri::is_valid_redirect_uri,
};

use crate::AppState;
use crate::response::{json_response, oauth_error};

pub async fn handle_register(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> impl IntoResponse {
    let Some(client_name) = body.client_name.filter(|n| !n.is_empty()) else {
        return oauth_error(OAuthError::InvalidClientMetadata(
            "client_name and redirect_uris are required".into(),
        ))
        .into_response();
    };

    let Some(redirect_uris) = body.redirect_uris.filter(|u| !u.is_empty()) else {
        return oauth_error(OAuthError::InvalidClientMetadata(
            "client_name and redirect_uris are required".into(),
        ))
        .into_response();
    };

    for uri in &redirect_uris {
        if !is_valid_redirect_uri(uri) {
            return oauth_error(OAuthError::InvalidRedirectUri(format!(
                "Invalid redirect_uri: {uri}"
            )))
            .into_response();
        }
    }

    let server_secret = match state.server_secret().await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to get server secret: {e}");
            return oauth_error(OAuthError::ServerError("Internal server error".into()))
                .into_response();
        }
    };

    let (client_id, client_secret) =
        generate_client_credentials(&client_name, &redirect_uris, server_secret);

    json_response(
        201,
        RegisterResponse {
            client_id,
            client_secret,
            client_name,
            redirect_uris,
            grant_types: vec!["authorization_code".into(), "refresh_token".into()],
            response_types: vec!["code".into()],
            token_endpoint_auth_method: "client_secret_basic".into(),
        },
    )
    .into_response()
}
