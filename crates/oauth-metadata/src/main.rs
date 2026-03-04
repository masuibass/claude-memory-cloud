use axum::{
    Router,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use lambda_http::{run, Error};
use serde::Serialize;
use std::env;

#[derive(Clone)]
struct AppState {
    api_url: String,
}

#[derive(Serialize)]
struct AuthorizationServerMetadata {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    registration_endpoint: String,
    response_types_supported: Vec<&'static str>,
    grant_types_supported: Vec<&'static str>,
    code_challenge_methods_supported: Vec<&'static str>,
    token_endpoint_auth_methods_supported: Vec<&'static str>,
    scopes_supported: Vec<&'static str>,
}

#[derive(Serialize)]
struct ProtectedResourceMetadata {
    resource: String,
    authorization_servers: Vec<String>,
    scopes_supported: Vec<&'static str>,
}

fn json_cached(body: impl Serialize) -> impl IntoResponse {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        serde_json::to_string(&body).unwrap(),
    )
}

async fn authorization_server(State(state): State<AppState>) -> impl IntoResponse {
    json_cached(AuthorizationServerMetadata {
        issuer: state.api_url.clone(),
        authorization_endpoint: format!("{}/authorize", state.api_url),
        token_endpoint: format!("{}/token", state.api_url),
        registration_endpoint: format!("{}/register", state.api_url),
        response_types_supported: vec!["code"],
        grant_types_supported: vec!["authorization_code", "refresh_token"],
        code_challenge_methods_supported: vec!["S256"],
        token_endpoint_auth_methods_supported: vec!["client_secret_basic", "client_secret_post"],
        scopes_supported: vec!["openid", "email", "profile"],
    })
}

async fn protected_resource(State(state): State<AppState>) -> impl IntoResponse {
    json_cached(ProtectedResourceMetadata {
        resource: format!("{}/mcp", state.api_url),
        authorization_servers: vec![state.api_url.clone()],
        scopes_supported: vec!["openid", "email", "profile"],
    })
}

async fn not_found() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "Not Found")
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .without_time()
        .with_ansi(false)
        .init();

    let state = AppState {
        api_url: env::var("API_URL").expect("API_URL is required"),
    };

    let app = Router::new()
        .route(
            "/.well-known/oauth-authorization-server",
            get(authorization_server),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            get(protected_resource),
        )
        .fallback(not_found)
        .with_state(state);

    run(app).await
}
