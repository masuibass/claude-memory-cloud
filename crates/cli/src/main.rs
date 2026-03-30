mod config;
mod log;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "memory-cloud", about = "Claude Code shared memory CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Set API URL and fetch Cognito config
    Init {
        /// API base URL (e.g. https://xxx.execute-api.amazonaws.com)
        api_url: String,
    },
    /// Authenticate via Cognito (PKCE flow)
    Login,
    /// Remove stored tokens
    Logout,
    /// Remove all local config and tokens
    Reset,
    /// Show CLI log file path or tail logs
    Logs {
        /// Follow log output (tail -f)
        #[arg(short, long)]
        follow: bool,
    },
    /// Semantic search across all transcripts
    Recall {
        /// Search query
        query: String,
        /// Number of results
        #[arg(short = 'k', long, default_value = "5")]
        top_k: i32,
    },
    /// Transcript operations
    Transcript {
        #[command(subcommand)]
        action: TranscriptAction,
    },
    /// Session operations
    Sessions {
        #[command(subcommand)]
        action: SessionsAction,
    },
    /// Share management
    Shares {
        #[command(subcommand)]
        action: SharesAction,
    },
}

#[derive(Subcommand)]
enum TranscriptAction {
    /// Upload a transcript file
    Put {
        /// Path to the JSONL file
        file: String,
        /// Project identifier (directory name from ~/.claude/projects/)
        #[arg(short, long)]
        project: Option<String>,
    },
    /// Download a transcript
    Get {
        /// Session ID
        session_id: String,
        /// Project identifier
        #[arg(short, long)]
        project: String,
        /// Get raw JSONL instead of parsed Markdown
        #[arg(long)]
        raw: bool,
    },
    /// Bulk upload all transcripts from ~/.claude/projects
    BulkUpload {
        /// Path to ~/.claude/projects (defaults to ~/.claude/projects)
        #[arg(short, long)]
        path: Option<String>,
    },
}

#[derive(Subcommand)]
enum SessionsAction {
    /// List your sessions
    List,
}

#[derive(Subcommand)]
enum SharesAction {
    /// Share your transcripts with another user
    Add {
        /// Recipient's user ID (Cognito sub)
        recipient_id: String,
    },
    /// Revoke a share you received (stop seeing their transcripts)
    Remove {
        /// Owner's user ID whose share to revoke
        owner_id: String,
    },
    /// Revoke a share you gave (stop sharing your transcripts with them)
    Revoke {
        /// Recipient's user ID to revoke access from
        recipient_id: String,
    },
    /// List shares
    List,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    log::init();
    let cli = Cli::parse();
    log::info(&format!("command: {:?}", std::env::args().collect::<Vec<_>>()));
    let result = run(cli).await;
    if let Err(ref e) = result {
        log::error(&format!("{e}"));
    }
    result
}

async fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Command::Init { api_url } => cmd_init(&api_url).await,
        Command::Login => cmd_login().await,
        Command::Logout => cmd_logout(),
        Command::Reset => cmd_reset(),
        Command::Logs { follow } => cmd_logs(follow),
        Command::Recall { query, top_k } => cmd_recall(&query, top_k).await,
        Command::Transcript { action } => match action {
            TranscriptAction::Put { file, project } => cmd_transcript_put(&file, project.as_deref()).await,
            TranscriptAction::Get { session_id, project, raw } => cmd_transcript_get(&session_id, &project, raw).await,
            TranscriptAction::BulkUpload { path } => cmd_bulk_upload(path.as_deref()).await,
        },
        Command::Sessions { action } => match action {
            SessionsAction::List => cmd_sessions_list().await,
        },
        Command::Shares { action } => match action {
            SharesAction::Add { recipient_id } => cmd_shares_add(&recipient_id).await,
            SharesAction::Remove { owner_id } => cmd_shares_remove(&owner_id).await,
            SharesAction::Revoke { recipient_id } => cmd_shares_revoke(&recipient_id).await,
            SharesAction::List => cmd_shares_list().await,
        },
    }
}

// ---------- auth helper ----------

/// Make an authenticated request. On 401, refresh tokens and retry once.
async fn authed_request(
    build: impl Fn(&str) -> reqwest::RequestBuilder,
) -> Result<reqwest::Response, Box<dyn std::error::Error>> {
    let token = config::load_id_token()?;
    let resp = build(&token).send().await?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        eprintln!("Token expired, refreshing...");
        let new_token = config::refresh_tokens().await?;
        let resp = build(&new_token).send().await?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err("Unauthorized after refresh. Run `memory-cloud login`.".into());
        }
        return Ok(resp);
    }

    Ok(resp)
}

// ---------- init ----------

async fn cmd_init(api_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let url = api_url.trim_end_matches('/');
    let resp: serde_json::Value = reqwest::get(format!("{url}/config")).await?.json().await?;

    let cognito_domain = resp["cognito_domain"]
        .as_str()
        .ok_or("missing cognito_domain")?;
    let client_id = resp["client_id"].as_str().ok_or("missing client_id")?;

    let cfg = config::Config {
        api_url: url.to_string(),
        cognito_domain: cognito_domain.to_string(),
        client_id: client_id.to_string(),
    };
    config::save_config(&cfg)?;
    println!("Config saved.");
    println!("  API: {url}");
    println!("  Cognito: {cognito_domain}");
    println!("  Client ID: {client_id}");
    println!("\nRun `memory-cloud login` to authenticate.");
    Ok(())
}

// ---------- logout ----------

fn cmd_logout() -> Result<(), Box<dyn std::error::Error>> {
    let tokens_path = config::tokens_path();
    if tokens_path.exists() {
        std::fs::remove_file(&tokens_path)?;
        println!("Logged out.");
    } else {
        println!("Not logged in.");
    }
    Ok(())
}

// ---------- logs ----------

fn cmd_logs(follow: bool) -> Result<(), Box<dyn std::error::Error>> {
    let path = log::log_path();
    if !path.exists() {
        println!("No logs yet: {}", path.display());
        return Ok(());
    }
    if follow {
        let status = std::process::Command::new("tail")
            .args(["-f", &path.to_string_lossy()])
            .status()?;
        std::process::exit(status.code().unwrap_or(0));
    } else {
        print!("{}", std::fs::read_to_string(&path)?);
    }
    Ok(())
}

// ---------- reset ----------

fn cmd_reset() -> Result<(), Box<dyn std::error::Error>> {
    let config_dir = config::config_dir_path();
    if config_dir.exists() {
        std::fs::remove_dir_all(&config_dir)?;
        println!("Removed {}", config_dir.display());
    } else {
        println!("Nothing to reset.");
    }
    Ok(())
}

// ---------- login ----------

async fn cmd_login() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load_config()?;

    let verifier = pkce_verifier();
    let challenge = pkce_challenge(&verifier);

    let server =
        tiny_http::Server::http("127.0.0.1:8976").map_err(|e| format!("bind failed: {e}"))?;

    let auth_url = format!(
        "https://{}/oauth2/authorize?response_type=code&client_id={}&redirect_uri={}&scope=openid+email+profile&code_challenge={}&code_challenge_method=S256",
        cfg.cognito_domain,
        cfg.client_id,
        "http://localhost:8976/callback",
        challenge,
    );

    println!("Opening browser for authentication...");
    if open::that(&auth_url).is_err() {
        println!("Open this URL in your browser:\n{auth_url}");
    }

    let request = server.recv()?;
    let url = request.url().to_string();
    let code = url
        .split("code=")
        .nth(1)
        .and_then(|s| s.split('&').next())
        .ok_or("no code in callback")?
        .to_string();

    let response = tiny_http::Response::from_string(
        "<html><body><h2>Login successful!</h2><p>You can close this tab.</p></body></html>",
    )
    .with_header("Content-Type: text/html".parse::<tiny_http::Header>().unwrap());
    let _ = request.respond(response);

    let client = reqwest::Client::new();
    let token_resp = client
        .post(format!("https://{}/oauth2/token", cfg.cognito_domain))
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", &cfg.client_id),
            ("code", &code),
            ("redirect_uri", "http://localhost:8976/callback"),
            ("code_verifier", &verifier),
        ])
        .send()
        .await?;

    if !token_resp.status().is_success() {
        let body = token_resp.text().await?;
        return Err(format!("Token exchange failed: {body}").into());
    }

    let tokens: serde_json::Value = token_resp.json().await?;
    config::save_tokens(&tokens)?;

    println!("Login successful!");
    Ok(())
}

fn pkce_verifier() -> String {
    use rand::Rng;
    let bytes: Vec<u8> = (0..32).map(|_| rand::rng().random::<u8>()).collect();
    base64_url_encode(&bytes)
}

fn pkce_challenge(verifier: &str) -> String {
    use sha2::Digest;
    let hash = sha2::Sha256::digest(verifier.as_bytes());
    base64_url_encode(&hash)
}

fn base64_url_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

// ---------- recall ----------

async fn cmd_recall(query: &str, top_k: i32) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load_config()?;
    let client = reqwest::Client::new();

    let url = format!("{}/recall", cfg.api_url);
    let body = serde_json::json!({ "query": query, "top_k": top_k });

    let resp = authed_request(|token| {
        client.post(&url).bearer_auth(token).json(&body)
    })
    .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await?;
        return Err(format!("{status}: {body}").into());
    }

    let body: serde_json::Value = resp.json().await?;
    let results = body["results"].as_array().ok_or("unexpected response")?;

    if results.is_empty() {
        println!("No results found.");
        return Ok(());
    }

    for r in results {
        let session_id = r["session_id"].as_str().unwrap_or("?");
        let score = r["score"].as_f64().unwrap_or(0.0);
        let text = r["text"].as_str().unwrap_or("");
        let preview: String = text.chars().take(200).collect();
        let meta = &r["metadata"];
        let project = meta["project"].as_str().unwrap_or("");
        let created_at = meta["created_at"].as_str().unwrap_or("");
        println!("--- {session_id} (score: {score:.4}) ---");
        if !project.is_empty() || !created_at.is_empty() {
            println!("  project: {project}  created: {created_at}");
        }
        println!("{preview}");
        println!();
    }
    Ok(())
}

// ---------- transcript put ----------

/// Derive project hash from a file path under ~/.claude/projects/{project_hash}/.
fn derive_project_from_path(file: &str) -> Option<String> {
    let path = std::path::Path::new(file);
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    let s = abs.to_str()?;

    // Look for the pattern: .claude/projects/{project_hash}/
    let marker = ".claude/projects/";
    let idx = s.find(marker)?;
    let after = &s[idx + marker.len()..];
    let project_hash = after.split('/').next()?;
    if project_hash.is_empty() {
        return None;
    }
    Some(project_hash.to_string())
}

async fn cmd_transcript_put(
    file: &str,
    project_arg: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load_config()?;
    let client = reqwest::Client::new();

    let path = std::path::Path::new(file);
    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("invalid file path")?
        .to_string();

    let project = match project_arg {
        Some(p) => p.to_string(),
        None => derive_project_from_path(file)
            .ok_or("cannot derive project from path; use --project")?,
    };

    let url = format!("{}/transcript", cfg.api_url);
    let body = serde_json::json!({ "session_id": session_id, "project": project });

    let resp = authed_request(|token| {
        client.post(&url).bearer_auth(token).json(&body)
    })
    .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await?;
        return Err(format!("{status}: {body}").into());
    }

    let resp_body: serde_json::Value = resp.json().await?;
    let upload_url = resp_body["upload_url"]
        .as_str()
        .ok_or("no upload_url")?;

    let file_bytes = std::fs::read(file)?;
    let size = file_bytes.len();
    let put_resp = client.put(upload_url).body(file_bytes).send().await?;

    if !put_resp.status().is_success() {
        let status = put_resp.status();
        let body = put_resp.text().await?;
        return Err(format!("Upload failed: {status}: {body}").into());
    }

    println!("Uploaded {session_id} ({size} bytes)");
    Ok(())
}

// ---------- transcript get ----------

async fn cmd_transcript_get(session_id: &str, project: &str, raw: bool) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load_config()?;
    let token = config::load_id_token()?;
    let user_id = config::extract_sub_from_token(&token)?;
    let client = reqwest::Client::new();

    let mut url = format!("{}/transcript/{}/{}/{}", cfg.api_url, user_id, project, session_id);
    if raw {
        url.push_str("?raw=true");
    }

    let resp = authed_request(|token| {
        client.get(&url).bearer_auth(token)
    })
    .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await?;
        return Err(format!("{status}: {body}").into());
    }

    let body: serde_json::Value = resp.json().await?;
    let download_url = body["download_url"].as_str().ok_or("no download_url")?;

    let content = client.get(download_url).send().await?.text().await?;
    print!("{content}");
    Ok(())
}

// ---------- sessions list ----------

async fn cmd_sessions_list() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load_config()?;
    let client = reqwest::Client::new();

    let url = format!("{}/sessions", cfg.api_url);

    let resp = authed_request(|token| {
        client.get(&url).bearer_auth(token)
    })
    .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await?;
        return Err(format!("{status}: {body}").into());
    }

    let body: serde_json::Value = resp.json().await?;
    let sessions = body["sessions"].as_array().ok_or("unexpected response")?;

    if sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    println!("{:<40} {:>10}  {}", "SESSION_ID", "SIZE", "LAST_MODIFIED");
    for s in sessions {
        let id = s["session_id"].as_str().unwrap_or("?");
        let size = s["size"].as_i64().unwrap_or(0);
        let modified = s["last_modified"].as_str().unwrap_or("?");
        println!("{:<40} {:>10}  {}", id, format_size(size), modified);
    }
    Ok(())
}

fn format_size(bytes: i64) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

// ---------- shares add ----------

async fn cmd_shares_add(recipient_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load_config()?;
    let client = reqwest::Client::new();

    let url = format!("{}/shares", cfg.api_url);
    let body = serde_json::json!({ "recipient_id": recipient_id });

    let resp = authed_request(|token| {
        client.post(&url).bearer_auth(token).json(&body)
    })
    .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await?;
        return Err(format!("{status}: {body}").into());
    }

    println!("Shared your transcripts with {recipient_id}");
    Ok(())
}

// ---------- shares remove ----------

async fn cmd_shares_remove(owner_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load_config()?;
    let client = reqwest::Client::new();

    let url = format!("{}/shares/{}", cfg.api_url, owner_id);

    let resp = authed_request(|token| {
        client.delete(&url).bearer_auth(token)
    })
    .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await?;
        return Err(format!("{status}: {body}").into());
    }

    println!("Removed share from {owner_id}");
    Ok(())
}

// ---------- shares revoke ----------

async fn cmd_shares_revoke(recipient_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load_config()?;
    let client = reqwest::Client::new();

    let url = format!("{}/shares/recipients/{}", cfg.api_url, recipient_id);

    let resp = authed_request(|token| {
        client.delete(&url).bearer_auth(token)
    })
    .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await?;
        return Err(format!("{status}: {body}").into());
    }

    println!("Revoked share to {recipient_id}");
    Ok(())
}

// ---------- shares list ----------

async fn cmd_shares_list() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load_config()?;
    let client = reqwest::Client::new();

    let url = format!("{}/shares", cfg.api_url);

    let resp = authed_request(|token| {
        client.get(&url).bearer_auth(token)
    })
    .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await?;
        return Err(format!("{status}: {body}").into());
    }

    let body: serde_json::Value = resp.json().await?;

    let shared_with_me = body["shared_with_me"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
        .unwrap_or_default();

    let shared_by_me = body["shared_by_me"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
        .unwrap_or_default();

    if shared_with_me.is_empty() && shared_by_me.is_empty() {
        println!("No shares found.");
        return Ok(());
    }

    if !shared_with_me.is_empty() {
        println!("Shared with me (I can search their transcripts):");
        for id in &shared_with_me {
            println!("  {id}");
        }
    }

    if !shared_by_me.is_empty() {
        println!("Shared by me (they can search my transcripts):");
        for id in &shared_by_me {
            println!("  {id}");
        }
    }

    Ok(())
}

// ---------- bulk upload ----------

async fn cmd_bulk_upload(path: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::var("HOME")?;
    let base = match path {
        Some(p) => std::path::PathBuf::from(p),
        None => std::path::PathBuf::from(&home).join(".claude/projects"),
    };

    if !base.is_dir() {
        return Err(format!("{} is not a directory", base.display()).into());
    }

    // Collect all JSONL files: {base}/{project_hash}/{session_id}.jsonl
    let mut files: Vec<(String, String)> = Vec::new();

    for project_entry in std::fs::read_dir(&base)? {
        let project_dir = project_entry?.path();
        if !project_dir.is_dir() {
            continue;
        }
        let project_hash = project_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if project_hash.is_empty() {
            continue;
        }

        for entry in std::fs::read_dir(&project_dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                files.push((project_hash.clone(), path.to_string_lossy().to_string()));
            }
        }
    }

    if files.is_empty() {
        println!("No JSONL files found in {}", base.display());
        return Ok(());
    }

    println!("Found {} files to upload", files.len());

    let cfg = config::load_config()?;
    let client = reqwest::Client::new();
    let mut uploaded = 0u64;
    let mut failed = 0u64;

    for (i, (project, file_path)) in files.iter().enumerate() {
        let path = std::path::Path::new(file_path);
        let session_id = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s,
            None => {
                failed += 1;
                continue;
            }
        };

        let url = format!("{}/transcript", cfg.api_url);
        let body = serde_json::json!({ "session_id": session_id, "project": project });

        let resp = match authed_request(|token| {
            client.post(&url).bearer_auth(token).json(&body)
        })
        .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[{}/{}] {} — API error: {e}", i + 1, files.len(), session_id);
                failed += 1;
                continue;
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            eprintln!("[{}/{}] {} — {status}", i + 1, files.len(), session_id);
            failed += 1;
            continue;
        }

        let resp_body: serde_json::Value = resp.json().await?;
        let upload_url = match resp_body["upload_url"].as_str() {
            Some(u) => u.to_string(),
            None => {
                eprintln!("[{}/{}] {} — no upload_url", i + 1, files.len(), session_id);
                failed += 1;
                continue;
            }
        };

        let file_bytes = std::fs::read(file_path)?;
        let size = file_bytes.len();

        match client.put(&upload_url).body(file_bytes).send().await {
            Ok(r) if r.status().is_success() => {
                println!("[{}/{}] {} ({}) done", i + 1, files.len(), session_id, format_size(size as i64));
                uploaded += 1;
            }
            Ok(r) => {
                eprintln!("[{}/{}] {} — upload failed: {}", i + 1, files.len(), session_id, r.status());
                failed += 1;
            }
            Err(e) => {
                eprintln!("[{}/{}] {} — upload error: {e}", i + 1, files.len(), session_id);
                failed += 1;
            }
        }
    }

    println!("\nDone: {uploaded} uploaded, {failed} failed");
    Ok(())
}
