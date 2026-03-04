use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// API Gateway JWT Authorizer が検証済みの JWT から account_id (Cognito `sub`) を抽出する。
/// Lambda event の header `x-amzn-oidc-identity` は HTTP API では使えないので、
/// Authorization ヘッダーの JWT payload を直接デコードする。
/// 注意: JWT の署名検証は API Gateway が済ませているため、ここでは行わない。
pub fn extract_account_id(auth_header: &str) -> Option<String> {
    let token = auth_header.strip_prefix("Bearer ").unwrap_or(auth_header);
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }

    let payload = URL_SAFE_NO_PAD.decode(parts[1]).ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&payload).ok()?;
    claims["sub"].as_str().map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    #[test]
    fn test_extract_account_id() {
        let header = r#"{"alg":"RS256"}"#;
        let payload = r#"{"sub":"user-123","email":"test@example.com"}"#;
        let sig = "fake-signature";

        let h = URL_SAFE_NO_PAD.encode(header);
        let p = URL_SAFE_NO_PAD.encode(payload);
        let token = format!("Bearer {h}.{p}.{sig}");

        assert_eq!(extract_account_id(&token), Some("user-123".to_string()));
    }
}
