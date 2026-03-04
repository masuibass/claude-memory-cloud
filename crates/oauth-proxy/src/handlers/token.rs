use axum::{
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use claude_memory_common::{dcr::verify_client_credentials, error::OAuthError};

use crate::AppState;
use crate::response::oauth_error;

pub async fn handle_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    // Parse client credentials from Authorization header or body
    let mut client_id: Option<String> = None;
    let mut client_secret: Option<String> = None;

    if let Some(auth) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        if let Some(encoded) = auth.strip_prefix("Basic ") {
            if let Ok(decoded) = STANDARD.decode(encoded) {
                if let Ok(decoded_str) = String::from_utf8(decoded) {
                    if let Some(colon_idx) = decoded_str.find(':') {
                        client_id = urlencoding::decode(&decoded_str[..colon_idx])
                            .ok()
                            .map(|s| s.into_owned());
                        client_secret = urlencoding::decode(&decoded_str[colon_idx + 1..])
                            .ok()
                            .map(|s| s.into_owned());
                    }
                }
            }
        }
    }

    // Parse body (application/x-www-form-urlencoded)
    let form: Vec<(String, String)> = serde_urlencoded::from_str(&body).unwrap_or_default();

    if client_id.is_none() {
        client_id = form
            .iter()
            .find(|(k, _)| k == "client_id")
            .map(|(_, v)| v.clone());
    }
    if client_secret.is_none() {
        client_secret = form
            .iter()
            .find(|(k, _)| k == "client_secret")
            .map(|(_, v)| v.clone());
    }

    let (Some(client_id), Some(client_secret)) = (client_id, client_secret) else {
        return oauth_error(OAuthError::InvalidClient(
            "Client credentials required".into(),
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

    if verify_client_credentials(&client_id, &client_secret, server_secret).is_none() {
        return oauth_error(OAuthError::InvalidClient(
            "Invalid client credentials".into(),
        ))
        .into_response();
    }

    // Get Cognito client secret
    let cognito_client_secret = match state.cognito_client_secret().await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to get Cognito client secret: {e}");
            return oauth_error(OAuthError::ServerError("Internal server error".into()))
                .into_response();
        }
    };

    // Build Cognito token request
    let grant_type = form
        .iter()
        .find(|(k, _)| k == "grant_type")
        .map(|(_, v)| v.as_str())
        .unwrap_or("authorization_code");

    let mut cognito_params = vec![
        ("grant_type", grant_type.to_string()),
        ("client_id", state.config.cognito_client_id.clone()),
        ("client_secret", cognito_client_secret.to_string()),
        (
            "redirect_uri",
            format!("{}/callback", state.config.api_url),
        ),
    ];

    for (key, value) in &form {
        match key.as_str() {
            "code" | "code_verifier" | "refresh_token" => {
                cognito_params.push((key, value.clone()));
            }
            _ => {}
        }
    }

    let cognito_body = serde_urlencoded::to_string(&cognito_params).unwrap();
    let cognito_url = format!("https://{}/oauth2/token", state.config.cognito_domain);

    let cognito_res: reqwest::Response = match state
        .http_client
        .post(&cognito_url)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(cognito_body)
        .send()
        .await
    {
        Ok(res) => res,
        Err(e) => {
            tracing::error!("Cognito token request failed: {e}");
            return oauth_error(OAuthError::ServerError("Internal server error".into()))
                .into_response();
        }
    };

    let status = cognito_res.status().as_u16();
    let response_body = cognito_res.text().await.unwrap_or_default();

    if status != 200 {
        tracing::warn!("Cognito token error: {status} {response_body}");
    }

    (
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        [(header::CONTENT_TYPE, "application/json")],
        response_body,
    )
        .into_response()
}
