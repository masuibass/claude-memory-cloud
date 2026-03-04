use serde::{Deserialize, Serialize};

// ========== DCR ==========

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub client_name: Option<String>,
    pub redirect_uris: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub client_id: String,
    pub client_secret: String,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    pub grant_types: Vec<String>,
    pub response_types: Vec<String>,
    pub token_endpoint_auth_method: String,
}

// ========== OAuth Error ==========

#[derive(Debug, Serialize)]
pub struct OAuthErrorResponse {
    pub error: String,
    pub error_description: String,
}

// ========== Proxy State ==========

#[derive(Debug, Serialize, Deserialize)]
pub struct ProxyState {
    pub redirect_uri: String,
    pub state: Option<String>,
    pub client_id: String,
}
