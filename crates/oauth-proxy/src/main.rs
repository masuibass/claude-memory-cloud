mod handlers;
mod response;
mod routes;

use aws_sdk_cognitoidentityprovider::Client as CognitoClient;
use aws_sdk_secretsmanager::Client as SmClient;
use lambda_http::{run, Error};
use claude_memory_common::{config::OAuthProxyConfig, secrets};

#[derive(Clone)]
pub struct AppState {
    pub config: OAuthProxyConfig,
    pub sm_client: SmClient,
    pub cognito_client: CognitoClient,
    pub http_client: reqwest::Client,
}

impl AppState {
    pub async fn server_secret(&self) -> Result<&'static str, secrets::SecretError> {
        secrets::get_server_secret(&self.sm_client, &self.config).await
    }

    pub async fn cognito_client_secret(&self) -> Result<&'static str, secrets::SecretError> {
        secrets::get_cognito_client_secret(&self.cognito_client, &self.config).await
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .without_time()
        .with_ansi(false)
        .init();

    let config = OAuthProxyConfig::from_env();
    let aws_config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;

    let state = AppState {
        config,
        sm_client: SmClient::new(&aws_config),
        cognito_client: CognitoClient::new(&aws_config),
        http_client: reqwest::Client::new(),
    };

    let app = routes::router().with_state(state);

    run(app).await
}
