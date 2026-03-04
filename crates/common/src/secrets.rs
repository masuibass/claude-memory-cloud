use aws_sdk_cognitoidentityprovider::Client as CognitoClient;
use aws_sdk_secretsmanager::Client as SmClient;
use tokio::sync::OnceCell;

use crate::config::OAuthProxyConfig;

static SERVER_SECRET: OnceCell<String> = OnceCell::const_new();
static COGNITO_CLIENT_SECRET: OnceCell<String> = OnceCell::const_new();

/// Get the server secret from Secrets Manager (cached after first call).
pub async fn get_server_secret(
    sm_client: &SmClient,
    config: &OAuthProxyConfig,
) -> Result<&'static str, SecretError> {
    SERVER_SECRET
        .get_or_try_init(|| async {
            let res = sm_client
                .get_secret_value()
                .secret_id(&config.secret_arn)
                .send()
                .await
                .map_err(|e| SecretError(format!("Failed to get secret: {e}")))?;

            let secret_string = res
                .secret_string()
                .ok_or_else(|| SecretError("Secret has no string value".into()))?;

            let parsed: serde_json::Value = serde_json::from_str(secret_string)
                .map_err(|e| SecretError(format!("Failed to parse secret JSON: {e}")))?;

            parsed["serverSecret"]
                .as_str()
                .map(String::from)
                .ok_or_else(|| SecretError("serverSecret not found in secret".into()))
        })
        .await
        .map(|s| s.as_str())
}

/// Get the Cognito app client secret via DescribeUserPoolClient (cached after first call).
pub async fn get_cognito_client_secret(
    cognito_client: &CognitoClient,
    config: &OAuthProxyConfig,
) -> Result<&'static str, SecretError> {
    COGNITO_CLIENT_SECRET
        .get_or_try_init(|| async {
            let res = cognito_client
                .describe_user_pool_client()
                .user_pool_id(&config.cognito_user_pool_id)
                .client_id(&config.cognito_client_id)
                .send()
                .await
                .map_err(|e| SecretError(format!("Failed to describe user pool client: {e}")))?;

            res.user_pool_client()
                .and_then(|c| c.client_secret().map(String::from))
                .ok_or_else(|| SecretError("Cognito client secret not found".into()))
        })
        .await
        .map(|s| s.as_str())
}

#[derive(Debug)]
pub struct SecretError(pub String);

impl std::fmt::Display for SecretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SecretError {}
