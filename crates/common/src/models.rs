use chrono::{DateTime, Utc};
use pgvector::Vector;
use serde::{Deserialize, Serialize};

// ========== Session ==========

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Session {
    pub id: String,
    pub account_id: String,
    pub project: String,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
}

// ========== Turn ==========

#[derive(Debug, Serialize, Deserialize)]
pub struct TurnInput {
    pub session_id: String,
    pub project: String,
    pub turn_number: i32,
    pub user_input: Option<String>,
    pub tools_used: Option<String>,
    pub ai_response: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Turn {
    pub id: i64,
    pub account_id: String,
    pub session_id: String,
    pub project: String,
    pub turn_number: Option<i32>,
    pub user_input: Option<String>,
    pub tools_used: Option<String>,
    pub ai_response: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

// ========== Transcript ==========

#[derive(Debug, Serialize)]
pub struct PresignedUrlResponse {
    pub url: String,
    pub s3_key: String,
}

#[derive(Debug, Deserialize)]
pub struct TranscriptUploadRequest {
    pub session_id: String,
    pub project: String,
    pub size_bytes: Option<i64>,
}

// ========== Transcript Parsing (used by transcript.rs / MCP server) ==========

#[derive(Debug, Deserialize)]
pub struct TranscriptEntry {
    #[serde(default)]
    pub r#type: String,
    pub timestamp: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    pub message: Option<TranscriptMessage>,
}

#[derive(Debug, Deserialize)]
pub struct TranscriptMessage {
    pub content: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ParsedToolResult {
    pub tool_use_id: String,
    pub content: String,
}

// ========== Embedding helper type ==========

#[derive(Debug)]
pub struct EmbeddingVec(pub Vector);
