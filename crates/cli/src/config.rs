use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub api_url: String,
    pub cognito_domain: String,
    pub client_id: String,
}

fn config_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = dirs::config_dir()
        .ok_or("cannot find config dir")?
        .join("memory-cloud");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn save_config(cfg: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let path = config_dir()?.join("config.toml");
    let content = toml::to_string_pretty(cfg)?;
    std::fs::write(&path, content)?;
    Ok(())
}

pub fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    let path = config_dir()?.join("config.toml");
    let content = std::fs::read_to_string(&path)
        .map_err(|_| "Not configured. Run `memory-cloud init <api_url>` first.")?;
    let cfg: Config = toml::from_str(&content)?;
    Ok(cfg)
}

pub fn tokens_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("memory-cloud/tokens.json")
}

pub fn save_tokens(tokens: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let path = config_dir()?.join("tokens.json");
    let content = serde_json::to_string_pretty(tokens)?;
    std::fs::write(&path, &content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn load_id_token() -> Result<String, Box<dyn std::error::Error>> {
    let path = config_dir()?.join("tokens.json");
    let content = std::fs::read_to_string(&path)
        .map_err(|_| "Not logged in. Run `memory-cloud login` first.")?;
    let tokens: serde_json::Value = serde_json::from_str(&content)?;

    let token = tokens["id_token"]
        .as_str()
        .or_else(|| tokens["access_token"].as_str())
        .ok_or("no token found in tokens.json")?;
    Ok(token.to_string())
}

/// Try to refresh tokens using the stored refresh_token.
/// Returns the new id_token on success.
pub async fn refresh_tokens() -> Result<String, Box<dyn std::error::Error>> {
    let cfg = load_config()?;
    let path = config_dir()?.join("tokens.json");
    let content = std::fs::read_to_string(&path)
        .map_err(|_| "Not logged in. Run `memory-cloud login` first.")?;
    let tokens: serde_json::Value = serde_json::from_str(&content)?;

    let refresh_token = tokens["refresh_token"]
        .as_str()
        .ok_or("no refresh_token. Run `memory-cloud login` again.")?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("https://{}/oauth2/token", cfg.cognito_domain))
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", cfg.client_id.as_str()),
            ("refresh_token", refresh_token),
        ])
        .send()
        .await?;

    if !resp.status().is_success() {
        let body = resp.text().await?;
        return Err(format!("Token refresh failed: {body}. Run `memory-cloud login` again.").into());
    }

    let mut new_tokens: serde_json::Value = resp.json().await?;

    // Cognito refresh response doesn't include refresh_token, keep the old one
    if new_tokens.get("refresh_token").is_none() {
        new_tokens["refresh_token"] = tokens["refresh_token"].clone();
    }

    save_tokens(&new_tokens)?;

    let token = new_tokens["id_token"]
        .as_str()
        .or_else(|| new_tokens["access_token"].as_str())
        .ok_or("no token in refresh response")?;
    Ok(token.to_string())
}

/// Extract the `sub` claim from a JWT without verification (for user_id).
pub fn extract_sub_from_token(token: &str) -> Result<String, Box<dyn std::error::Error>> {
    use base64::Engine;
    let payload = token.split('.').nth(1).ok_or("invalid JWT")?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload)?;
    let claims: serde_json::Value = serde_json::from_slice(&decoded)?;
    let sub = claims["sub"].as_str().ok_or("no sub in JWT")?;
    Ok(sub.to_string())
}
