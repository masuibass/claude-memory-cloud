use std::env;
use std::io::{BufRead, BufReader};

use aws_lambda_events::event::sqs::SqsEvent;
use aws_sdk_s3::Client as S3Client;
use lambda_runtime::{Error, LambdaEvent, service_fn};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use transcript::{
    AssistantLine, ContentBlock, MessageContent, SystemLine, TranscriptLine, UserLine,
};

// ============================================================
// S3 Event (embedded in SQS body)
// ============================================================

#[derive(Deserialize)]
struct S3Event {
    #[serde(rename = "Records")]
    records: Vec<S3Record>,
}

#[derive(Deserialize)]
struct S3Record {
    s3: S3Info,
}

#[derive(Deserialize)]
struct S3Info {
    bucket: S3Bucket,
    object: S3Object,
}

#[derive(Deserialize)]
struct S3Bucket {
    name: String,
}

#[derive(Deserialize)]
struct S3Object {
    key: String,
}

// ============================================================
// Metadata for .metadata.json (Bedrock KB spec, ≤1KB for S3 Vectors)
// ============================================================

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KbMetadataFile {
    metadata_attributes: KbMetadataAttributes,
}

#[derive(Serialize)]
struct KbMetadataAttributes {
    user_id: String,
    project: String,
    session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<String>,
}

// ============================================================
// Parsed session state
// ============================================================

struct ParsedSession {
    markdown: String,
    user_id: String,
    project: String,
    session_id: String,
    title: Option<String>,
    created_at: Option<String>,
}

// ============================================================
// Main Lambda handler
// ============================================================

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .without_time()
        .with_ansi(false)
        .init();

    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let s3 = S3Client::new(&config);
    let parsed_bucket = env::var("PARSED_BUCKET").unwrap_or_default();

    lambda_runtime::run(service_fn(move |event: LambdaEvent<SqsEvent>| {
        let s3 = s3.clone();
        let parsed_bucket = parsed_bucket.clone();
        async move { handle_sqs(event.payload, &s3, &parsed_bucket).await }
    }))
    .await
}

async fn handle_sqs(event: SqsEvent, s3: &S3Client, parsed_bucket: &str) -> Result<(), Error> {
    for sqs_record in &event.records {
        let body = sqs_record.body.as_deref().unwrap_or("");
        let s3_event: S3Event = serde_json::from_str(body)?;

        for s3_record in &s3_event.records {
            let bucket = &s3_record.s3.bucket.name;
            let key = urlencoding::decode(&s3_record.s3.object.key)
                .unwrap_or_default()
                .into_owned();

            tracing::info!(bucket = bucket, key = key, "processing");

            if !key.ends_with(".jsonl") {
                tracing::info!(key = key, "skipping non-jsonl");
                continue;
            }

            match process_object(s3, bucket, &key, parsed_bucket).await {
                Ok(()) => tracing::info!(key = key, "done"),
                Err(e) => tracing::error!(key = key, error = %e, "failed"),
            }
        }
    }
    Ok(())
}

// ============================================================
// Core: S3 stream → parse → put
// ============================================================

async fn process_object(
    s3: &S3Client,
    raw_bucket: &str,
    key: &str,
    parsed_bucket: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Get JSONL from raw bucket
    let resp = s3.get_object().bucket(raw_bucket).key(key).send().await?;

    let body_bytes = resp.body.collect().await?.into_bytes();
    let reader = BufReader::new(body_bytes.as_ref());

    // 2. Parse key: {user_id}/{project_hash}/{session_id}.jsonl
    let (user_id, project, session_id) = parse_s3_key(key)?;

    // 3. Parse lines and build markdown
    let session = parse_lines(reader, user_id, project, session_id)?;

    // 4. Put markdown
    let md_key = format!(
        "{}/{}/{}.md",
        session.user_id, session.project, session.session_id
    );
    s3.put_object()
        .bucket(parsed_bucket)
        .key(&md_key)
        .body(session.markdown.into_bytes().into())
        .content_type("text/markdown; charset=utf-8")
        .send()
        .await?;

    // 5. Put metadata.json
    let metadata_key = format!("{}.metadata.json", md_key);
    let metadata = KbMetadataFile {
        metadata_attributes: KbMetadataAttributes {
            user_id: session.user_id,
            project: session.project,
            session_id: session.session_id,
            title: session.title,
            created_at: session.created_at,
        },
    };
    let metadata_json = serde_json::to_string(&metadata)?;
    s3.put_object()
        .bucket(parsed_bucket)
        .key(&metadata_key)
        .body(metadata_json.into_bytes().into())
        .content_type("application/json")
        .send()
        .await?;

    Ok(())
}

/// Parse S3 key into (user_id, project_hash, session_id).
fn parse_s3_key(key: &str) -> Result<(String, String, String), String> {
    // Expected: {user_id}/{project_hash}/{session_id}.jsonl
    let parts: Vec<&str> = key.splitn(3, '/').collect();
    if parts.len() < 3 {
        return Err(format!("unexpected key format: {key}"));
    }
    let user_id = parts[0].to_string();
    let project = parts[1].to_string();
    let session_id = parts[2]
        .strip_suffix(".jsonl")
        .ok_or_else(|| format!("key does not end with .jsonl: {key}"))?
        .to_string();
    Ok((user_id, project, session_id))
}

// ============================================================
// JSONL → Markdown conversion (line-by-line)
// ============================================================

fn parse_lines<R: BufRead>(
    reader: R,
    user_id: String,
    project: String,
    session_id: String,
) -> Result<ParsedSession, Box<dyn std::error::Error + Send + Sync>> {
    let mut md = String::with_capacity(8192);
    let mut title: Option<String> = None;
    let mut created_at: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut git_branch: Option<String> = None;
    let mut header_written = false;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let entry: TranscriptLine = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        match entry {
            TranscriptLine::User(ref u) => {
                if !header_written {
                    created_at = Some(u.timestamp.clone());
                    cwd.clone_from(&u.cwd);
                    git_branch.clone_from(&u.git_branch);
                    header_written = true;

                    md.push_str(&format!("# Session: {session_id}\n"));
                    if let Some(ref c) = cwd {
                        md.push_str(&format!("Project: {c}\n"));
                    }
                    if let Some(ref b) = git_branch {
                        md.push_str(&format!("Branch: {b}\n"));
                    }
                    md.push('\n');
                }
                render_user(&mut md, u);
            }
            TranscriptLine::Assistant(ref a) => {
                render_assistant(&mut md, a);
            }
            TranscriptLine::System(ref s) => {
                render_system(&mut md, s);
            }
            TranscriptLine::CustomTitle(ref ct) => {
                title = Some(ct.custom_title.clone());
            }
            _ => {}
        }
    }

    Ok(ParsedSession {
        markdown: md,
        user_id,
        project,
        session_id,
        title,
        created_at,
    })
}

// ============================================================
// Render functions
// ============================================================

fn render_user(md: &mut String, u: &UserLine) {
    if u.tool_use_result.is_some() || u.is_compact_summary == Some(true) || u.is_meta == Some(true)
    {
        return;
    }

    let text = extract_user_text(&u.message.content);
    if text.is_empty() {
        return;
    }

    md.push_str("## User\n");
    md.push_str(&escape_heading_markers(&text));
    md.push_str("\n\n");
}

fn render_assistant(md: &mut String, a: &AssistantLine) {
    if a.is_api_error_message == Some(true) {
        return;
    }

    let mut has_content = false;
    let mut section = String::new();

    for block in &a.message.content {
        match block {
            ContentBlock::Text { text } => {
                if !text.is_empty() {
                    section.push_str(&escape_heading_markers(text));
                    section.push('\n');
                    has_content = true;
                }
            }
            ContentBlock::ToolUse { name, input, .. } => {
                let params = summarize_tool_input(name, input);
                section.push_str(&format!("[{name}: {params}]\n"));
                has_content = true;
            }
            ContentBlock::Thinking { .. } => {}
            _ => {}
        }
    }

    if has_content {
        md.push_str("## Assistant\n");
        md.push_str(&section);
        md.push('\n');
    }
}

fn render_system(md: &mut String, s: &SystemLine) {
    if matches!(s.subtype, transcript::SystemSubtype::CompactBoundary) {
        if let Some(ref content) = s.content {
            md.push_str("## Summary (compacted)\n");
            md.push_str(&escape_heading_markers(content));
            md.push_str("\n\n");
        }
    }
}

// ============================================================
// Helpers
// ============================================================

/// Escape `#` at the start of lines so content doesn't create false
/// Markdown heading boundaries that confuse semantic chunking.
fn escape_heading_markers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.lines().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if line.starts_with('#') {
            out.push('\\');
        }
        out.push_str(line);
    }
    out
}

fn extract_user_text(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(s) => s.clone(),
        MessageContent::Blocks(blocks) => {
            let mut parts = Vec::new();
            for block in blocks {
                if let ContentBlock::Text { text } = block {
                    parts.push(text.as_str());
                }
            }
            parts.join("")
        }
    }
}

/// Summarize tool input parameters for the markdown output.
fn summarize_tool_input(tool_name: &str, input: &Value) -> String {
    let obj = match input.as_object() {
        Some(o) => o,
        None => return String::new(),
    };

    match tool_name {
        "Read" => {
            let path = obj.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
            let offset = obj.get("offset").and_then(|v| v.as_i64());
            let limit = obj.get("limit").and_then(|v| v.as_i64());
            let mut s = format!("file={path}");
            if let Some(o) = offset {
                s.push_str(&format!(", offset={o}"));
            }
            if let Some(l) = limit {
                s.push_str(&format!(", limit={l}"));
            }
            s
        }
        "Edit" | "Write" => {
            let path = obj.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("file={path}")
        }
        "Bash" => {
            let cmd = obj.get("command").and_then(|v| v.as_str()).unwrap_or("?");
            let truncated = if cmd.len() > 200 { &cmd[..200] } else { cmd };
            format!("command={truncated}")
        }
        "Glob" | "Grep" => {
            let pattern = obj.get("pattern").and_then(|v| v.as_str()).unwrap_or("?");
            format!("pattern={pattern}")
        }
        "Agent" => {
            let desc = obj
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("description={desc}")
        }
        "WebSearch" => {
            let query = obj.get("query").and_then(|v| v.as_str()).unwrap_or("?");
            format!("query={query}")
        }
        "WebFetch" => {
            let url = obj.get("url").and_then(|v| v.as_str()).unwrap_or("?");
            format!("url={url}")
        }
        "Skill" => {
            let skill = obj.get("skill").and_then(|v| v.as_str()).unwrap_or("?");
            format!("skill={skill}")
        }
        _ => {
            // MCP and other tools: list keys with truncated values
            let pairs: Vec<String> = obj
                .iter()
                .map(|(k, v)| {
                    let val = match v {
                        Value::String(s) => {
                            if s.len() > 100 {
                                format!("{}...", &s[..100])
                            } else {
                                s.clone()
                            }
                        }
                        other => {
                            let s = other.to_string();
                            if s.len() > 100 {
                                format!("{}...", &s[..100])
                            } else {
                                s
                            }
                        }
                    };
                    format!("{k}={val}")
                })
                .collect();
            pairs.join(", ")
        }
    }
}
