use serde::{Deserialize, Serialize};

use crate::crypto::{decrypt, encrypt, hmac_sign, hmac_verify};
use crate::uri::normalize_redirect_uri;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClientPayload {
    client_name: String,
    redirect_uris: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub client_name: String,
    pub redirect_uris: Vec<String>,
}

/// Generate self-contained client credentials (client_id = encrypted payload, client_secret = HMAC).
pub fn generate_client_credentials(
    client_name: &str,
    redirect_uris: &[String],
    secret: &str,
) -> (String, String) {
    let mut normalized: Vec<String> = redirect_uris
        .iter()
        .map(|uri| normalize_redirect_uri(uri))
        .collect();
    normalized.sort();

    let payload = serde_json::to_string(&ClientPayload {
        client_name: client_name.to_string(),
        redirect_uris: normalized,
    })
    .expect("serialize payload");

    let client_id = encrypt(&payload, secret);
    let client_secret = hmac_sign(&client_id, secret);
    (client_id, client_secret)
}

/// Verify client_id + client_secret and return the decoded client info.
pub fn verify_client_credentials(
    client_id: &str,
    client_secret: &str,
    secret: &str,
) -> Option<ClientInfo> {
    if !hmac_verify(client_id, client_secret, secret) {
        return None;
    }
    verify_client_id(client_id, secret)
}

/// Verify and decode a client_id (without checking the secret).
pub fn verify_client_id(client_id: &str, secret: &str) -> Option<ClientInfo> {
    let plaintext = decrypt(client_id, secret).ok()?;
    let payload: ClientPayload = serde_json::from_str(&plaintext).ok()?;
    Some(ClientInfo {
        client_name: payload.client_name,
        redirect_uris: payload.redirect_uris,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_verify() {
        let secret = "test-server-secret-for-dcr-tests";
        let uris = vec!["http://127.0.0.1:3000/callback".to_string()];

        let (client_id, client_secret) =
            generate_client_credentials("Claude Code", &uris, secret);

        let info = verify_client_credentials(&client_id, &client_secret, secret).unwrap();
        assert_eq!(info.client_name, "Claude Code");
        // Port should be stripped in normalized form
        assert_eq!(info.redirect_uris, vec!["http://127.0.0.1/callback"]);
    }

    #[test]
    fn test_verify_wrong_secret() {
        let secret = "correct-secret";
        let uris = vec!["http://127.0.0.1/callback".to_string()];

        let (client_id, client_secret) =
            generate_client_credentials("Test", &uris, secret);

        assert!(verify_client_credentials(&client_id, &client_secret, "wrong-secret").is_none());
    }

    #[test]
    fn test_verify_tampered_client_secret() {
        let secret = "test-secret";
        let uris = vec!["http://127.0.0.1/callback".to_string()];

        let (client_id, _) = generate_client_credentials("Test", &uris, secret);

        assert!(verify_client_credentials(&client_id, "tampered", secret).is_none());
    }

    #[test]
    fn test_verify_client_id_only() {
        let secret = "test-secret";
        let uris = vec!["https://example.com/callback".to_string()];

        let (client_id, _) = generate_client_credentials("Test", &uris, secret);

        let info = verify_client_id(&client_id, secret).unwrap();
        assert_eq!(info.client_name, "Test");
    }
}
