use std::env;

#[derive(Debug, Clone)]
pub struct OAuthProxyConfig {
    pub cognito_user_pool_id: String,
    pub cognito_client_id: String,
    pub cognito_domain: String,
    pub api_url: String,
    pub secret_arn: String,
    pub region: String,
}

impl OAuthProxyConfig {
    pub fn from_env() -> Self {
        Self {
            cognito_user_pool_id: env::var("COGNITO_USER_POOL_ID")
                .expect("COGNITO_USER_POOL_ID is required"),
            cognito_client_id: env::var("COGNITO_CLIENT_ID")
                .expect("COGNITO_CLIENT_ID is required"),
            cognito_domain: env::var("COGNITO_DOMAIN").expect("COGNITO_DOMAIN is required"),
            api_url: env::var("API_URL").expect("API_URL is required"),
            secret_arn: env::var("SECRET_ARN").expect("SECRET_ARN is required"),
            region: env::var("REGION").expect("REGION is required"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MetadataConfig {
    pub api_url: String,
}

impl MetadataConfig {
    pub fn from_env() -> Self {
        Self {
            api_url: env::var("API_URL").expect("API_URL is required"),
        }
    }
}
