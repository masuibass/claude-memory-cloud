use url::Url;

/// Normalize localhost redirect URIs by stripping port (for deduplication).
pub fn normalize_redirect_uri(uri: &str) -> String {
    let Ok(url) = Url::parse(uri) else {
        return uri.to_string();
    };
    match url.host_str() {
        Some("127.0.0.1" | "localhost" | "::1") => {
            format!("{}://{}{}", url.scheme(), url.host_str().unwrap(), url.path())
        }
        _ => uri.to_string(),
    }
}

/// Validate that a redirect URI is safe (HTTPS or localhost HTTP).
pub fn is_valid_redirect_uri(uri: &str) -> bool {
    let Ok(url) = Url::parse(uri) else {
        return false;
    };
    match url.scheme() {
        "https" => true,
        "http" => matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1")),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_localhost_strips_port() {
        assert_eq!(
            normalize_redirect_uri("http://127.0.0.1:12345/callback"),
            "http://127.0.0.1/callback"
        );
        assert_eq!(
            normalize_redirect_uri("http://localhost:9999/callback"),
            "http://localhost/callback"
        );
    }

    #[test]
    fn test_normalize_https_preserves_url() {
        let uri = "https://example.com/callback";
        assert_eq!(normalize_redirect_uri(uri), uri);
    }

    #[test]
    fn test_valid_redirect_uri() {
        assert!(is_valid_redirect_uri("https://example.com/callback"));
        assert!(is_valid_redirect_uri("http://127.0.0.1:3000/callback"));
        assert!(is_valid_redirect_uri("http://localhost:8080/callback"));
        assert!(!is_valid_redirect_uri("http://evil.com/callback"));
        assert!(!is_valid_redirect_uri("ftp://example.com/file"));
    }
}
