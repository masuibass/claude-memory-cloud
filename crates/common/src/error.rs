use thiserror::Error;

#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("invalid_client_metadata: {0}")]
    InvalidClientMetadata(String),

    #[error("invalid_redirect_uri: {0}")]
    InvalidRedirectUri(String),

    #[error("invalid_client: {0}")]
    InvalidClient(String),

    #[error("invalid_request: {0}")]
    InvalidRequest(String),

    #[error("invalid_state: {0}")]
    InvalidState(String),

    #[error("server_error: {0}")]
    ServerError(String),
}

impl OAuthError {
    pub fn status_code(&self) -> u16 {
        match self {
            Self::InvalidClientMetadata(_) => 400,
            Self::InvalidRedirectUri(_) => 400,
            Self::InvalidRequest(_) => 400,
            Self::InvalidState(_) => 400,
            Self::InvalidClient(_) => 401,
            Self::ServerError(_) => 500,
        }
    }

    pub fn error_code(&self) -> &str {
        match self {
            Self::InvalidClientMetadata(_) => "invalid_client_metadata",
            Self::InvalidRedirectUri(_) => "invalid_redirect_uri",
            Self::InvalidRequest(_) => "invalid_request",
            Self::InvalidState(_) => "invalid_state",
            Self::InvalidClient(_) => "invalid_client",
            Self::ServerError(_) => "server_error",
        }
    }
}
