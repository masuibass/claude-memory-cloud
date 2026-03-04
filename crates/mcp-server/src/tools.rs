use std::collections::HashMap;
use std::sync::Arc;

use aws_sdk_bedrockruntime::Client as BedrockClient;
use aws_sdk_s3::Client as S3Client;
use chrono::{DateTime, Utc};
use futures::future::join_all;
use rmcp::{
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router, ServerHandler,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use claude_memory_common::embedding::embed_text;
use claude_memory_common::transcript::{
    extract_text, extract_tool_results, extract_tool_uses, fetch_transcript_bytes, parse_entries,
};

// ========== Input types ==========

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchMemoryInput {
    /// Search query text (used for both FTS and vector search)
    pub query: String,
    /// Optional project path filter
    pub project: Option<String>,
    /// Maximum number of results (default: 20, max: 100)
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSessionsInput {
    /// Optional project path filter
    pub project: Option<String>,
    /// Maximum number of sessions (default: 20)
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSessionTurnsInput {
    /// Session ID to get turns for
    pub session_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindSimilarInput {
    /// Text to find similar content for
    pub text: String,
    /// Optional project path filter
    pub project: Option<String>,
    /// Maximum number of results (default: 10)
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchTranscriptsInput {
    /// Search keyword (case-insensitive)
    pub q: String,
    /// Optional session ID to search within
    pub session_id: Option<String>,
    /// Maximum number of results (default: 20, max: 100)
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetTranscriptToolsInput {
    /// Optional session ID to filter
    pub session_id: Option<String>,
    /// Optional tool name filter (e.g. "Read", "Bash")
    pub tool_name: Option<String>,
    /// Include tool output in results (default: false)
    #[serde(default)]
    pub include_output: bool,
    /// Maximum number of results (default: 50, max: 200)
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetTranscriptInput {
    /// Session ID to get transcript for
    pub session_id: String,
    /// Include tool results in response (default: false)
    #[serde(default)]
    pub include_tool_results: bool,
}

// ========== Output types ==========

#[derive(Debug, Serialize, sqlx::FromRow)]
struct SearchResult {
    id: i64,
    session_id: String,
    project: String,
    turn_number: Option<i32>,
    user_input: Option<String>,
    ai_response: Option<String>,
    created_at: Option<DateTime<Utc>>,
    rrf_score: Option<f64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct SessionRow {
    id: String,
    project: String,
    started_at: Option<DateTime<Utc>>,
    turn_count: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct TurnRow {
    turn_number: Option<i32>,
    user_input: Option<String>,
    tools_used: Option<String>,
    ai_response: Option<String>,
    created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct SimilarRow {
    id: i64,
    session_id: String,
    project: String,
    user_input: Option<String>,
    ai_response: Option<String>,
    created_at: Option<DateTime<Utc>>,
    similarity: Option<f64>,
}

// ========== MCP Server ==========

#[derive(Debug, Clone)]
pub struct McpServer {
    pool: Arc<PgPool>,
    bedrock: Arc<BedrockClient>,
    s3: Arc<S3Client>,
    transcript_bucket: Arc<String>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl McpServer {
    pub fn new(
        pool: Arc<PgPool>,
        bedrock: Arc<BedrockClient>,
        s3: Arc<S3Client>,
        transcript_bucket: Arc<String>,
    ) -> Self {
        Self {
            pool,
            bedrock,
            s3,
            transcript_bucket,
            tool_router: Self::tool_router(),
        }
    }

    /// Hybrid search across all stored turns using both full-text search and vector similarity.
    /// Returns results ranked by Reciprocal Rank Fusion (RRF) score.
    #[tool(description = "Search cloud memory using hybrid FTS + vector search. Use this to find past conversations, solutions, and context across all projects.")]
    async fn search_memory(&self, Parameters(input): Parameters<SearchMemoryInput>) -> String {
        let limit = input.limit.unwrap_or(20).min(100);

        let embedding = match embed_text(&self.bedrock, &input.query).await {
            Ok(e) => e,
            Err(e) => return format!("Embedding error: {e}"),
        };

        let results: Result<Vec<SearchResult>, _> = sqlx::query_as(
            r#"
            WITH fts AS (
                SELECT id, ROW_NUMBER() OVER (ORDER BY ts_rank(search_vector, websearch_to_tsquery('simple', $1)) DESC) AS rank
                FROM turns
                WHERE search_vector @@ websearch_to_tsquery('simple', $1)
                  AND ($3::text IS NULL OR project = $3)
                LIMIT $2
            ),
            vec AS (
                SELECT id, ROW_NUMBER() OVER (ORDER BY embedding <=> $4 ASC) AS rank
                FROM turns
                WHERE embedding IS NOT NULL
                  AND ($3::text IS NULL OR project = $3)
                ORDER BY embedding <=> $4
                LIMIT $2
            ),
            rrf AS (
                SELECT
                    COALESCE(f.id, v.id) AS id,
                    (COALESCE(1.0 / (60 + f.rank), 0) + COALESCE(1.0 / (60 + v.rank), 0))::float8 AS rrf_score
                FROM fts f
                FULL OUTER JOIN vec v ON f.id = v.id
                ORDER BY rrf_score DESC
                LIMIT $2
            )
            SELECT t.id, t.session_id, t.project, t.turn_number,
                   LEFT(t.user_input, 200) AS user_input,
                   LEFT(t.ai_response, 200) AS ai_response,
                   t.created_at,
                   r.rrf_score
            FROM rrf r
            JOIN turns t ON t.id = r.id
            ORDER BY r.rrf_score DESC
            "#,
        )
        .bind(&input.query)
        .bind(limit)
        .bind(&input.project)
        .bind(&embedding)
        .fetch_all(self.pool.as_ref())
        .await;

        match results {
            Ok(rows) => serde_json::to_string_pretty(&rows).unwrap_or_else(|e| format!("JSON error: {e}")),
            Err(e) => format!("DB error: {e}"),
        }
    }

    /// Get recent sessions, optionally filtered by project.
    #[tool(description = "List recent sessions with turn counts. Use this to see what conversations happened recently.")]
    async fn get_sessions(&self, Parameters(input): Parameters<GetSessionsInput>) -> String {
        let limit = input.limit.unwrap_or(20).min(100);

        let results: Result<Vec<SessionRow>, _> = sqlx::query_as(
            r#"
            SELECT s.id, s.project, s.started_at,
                   (SELECT COUNT(*) FROM turns t WHERE t.session_id = s.id) AS turn_count
            FROM sessions s
            WHERE ($2::text IS NULL OR s.project = $2)
            ORDER BY s.started_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .bind(&input.project)
        .fetch_all(self.pool.as_ref())
        .await;

        match results {
            Ok(rows) => serde_json::to_string_pretty(&rows).unwrap_or_else(|e| format!("JSON error: {e}")),
            Err(e) => format!("DB error: {e}"),
        }
    }

    /// Get all turns for a specific session.
    #[tool(description = "Get all turns in a specific session. Use this to review the full conversation history of a past session.")]
    async fn get_session_turns(&self, Parameters(input): Parameters<GetSessionTurnsInput>) -> String {
        let results: Result<Vec<TurnRow>, _> = sqlx::query_as(
            r#"
            SELECT turn_number, user_input, tools_used, ai_response, created_at
            FROM turns
            WHERE session_id = $1
            ORDER BY turn_number ASC
            "#,
        )
        .bind(&input.session_id)
        .fetch_all(self.pool.as_ref())
        .await;

        match results {
            Ok(rows) => serde_json::to_string_pretty(&rows).unwrap_or_else(|e| format!("JSON error: {e}")),
            Err(e) => format!("DB error: {e}"),
        }
    }

    /// Find turns with semantically similar content using vector search.
    #[tool(description = "Find semantically similar past conversations using vector search. Use this when you want to find related context even if exact keywords don't match.")]
    async fn find_similar(&self, Parameters(input): Parameters<FindSimilarInput>) -> String {
        let limit = input.limit.unwrap_or(10).min(50);

        let embedding = match embed_text(&self.bedrock, &input.text).await {
            Ok(e) => e,
            Err(e) => return format!("Embedding error: {e}"),
        };

        let results: Result<Vec<SimilarRow>, _> = sqlx::query_as(
            r#"
            SELECT id, session_id, project,
                   LEFT(user_input, 200) AS user_input,
                   LEFT(ai_response, 200) AS ai_response,
                   created_at,
                   (1 - (embedding <=> $1))::float8 AS similarity
            FROM turns
            WHERE embedding IS NOT NULL
              AND ($3::text IS NULL OR project = $3)
            ORDER BY embedding <=> $1
            LIMIT $2
            "#,
        )
        .bind(&embedding)
        .bind(limit)
        .bind(&input.project)
        .fetch_all(self.pool.as_ref())
        .await;

        match results {
            Ok(rows) => serde_json::to_string_pretty(&rows).unwrap_or_else(|e| format!("JSON error: {e}")),
            Err(e) => format!("DB error: {e}"),
        }
    }

    // ========== Transcript tools ==========

    /// Helper: get account_id from the DB (single-user setup returns the first one)
    async fn get_account_id(&self) -> Result<String, String> {
        sqlx::query_scalar::<_, String>("SELECT DISTINCT account_id FROM transcripts LIMIT 1")
            .fetch_optional(self.pool.as_ref())
            .await
            .map_err(|e| format!("DB error: {e}"))?
            .ok_or_else(|| "No transcripts found".to_string())
    }

    /// Helper: get s3 keys, optionally filtered by session_id
    async fn get_s3_keys(
        &self,
        account_id: &str,
        session_id: Option<&str>,
        max_keys: i64,
    ) -> Result<Vec<String>, String> {
        if let Some(sid) = session_id {
            Ok(vec![format!("{account_id}/{sid}.jsonl")])
        } else {
            sqlx::query_scalar::<_, String>(
                "SELECT s3_key FROM transcripts WHERE account_id = $1 ORDER BY uploaded_at DESC LIMIT $2",
            )
            .bind(account_id)
            .bind(max_keys)
            .fetch_all(self.pool.as_ref())
            .await
            .map_err(|e| format!("DB error: {e}"))
        }
    }

    /// Helper: extract session_id from s3_key
    fn session_id_from_key(key: &str, account_id: &str) -> String {
        key.strip_prefix(&format!("{account_id}/"))
            .and_then(|s| s.strip_suffix(".jsonl"))
            .unwrap_or(key)
            .to_string()
    }

    /// Search transcript text content by keyword. Returns matching text snippets with session ID and timestamp.
    #[tool(description = "Search transcript text by keyword. Use this to find past conversations by content. Returns matching snippets from user and assistant messages.")]
    async fn search_transcripts(
        &self,
        Parameters(input): Parameters<SearchTranscriptsInput>,
    ) -> String {
        let account_id = match self.get_account_id().await {
            Ok(id) => id,
            Err(e) => return e,
        };

        let limit = input.limit.unwrap_or(20).min(100) as usize;
        let q_lower = input.q.to_lowercase();

        let s3_keys = match self.get_s3_keys(&account_id, input.session_id.as_deref(), 20).await {
            Ok(keys) => keys,
            Err(e) => return e,
        };

        let fetches = s3_keys
            .iter()
            .map(|key| fetch_transcript_bytes(&self.s3, &self.transcript_bucket, key));
        let all_bytes = join_all(fetches).await;

        #[derive(Serialize)]
        struct Hit {
            session_id: String,
            timestamp: Option<String>,
            source: String,
            text: String,
        }

        let mut results: Vec<Hit> = Vec::new();

        for (i, maybe_bytes) in all_bytes.into_iter().enumerate() {
            let Some(bytes) = maybe_bytes else { continue };
            let entries = parse_entries(&bytes);
            let session_id = Self::session_id_from_key(&s3_keys[i], &account_id);

            for entry in &entries {
                let Some(ref msg) = entry.message else {
                    continue;
                };
                let text = extract_text(&msg.content);
                if text.is_empty() || !text.to_lowercase().contains(&q_lower) {
                    continue;
                }

                let source = match entry.r#type.as_str() {
                    "human" => "user",
                    "assistant" => "assistant",
                    other => other,
                };

                let truncated = if text.len() > 500 {
                    let end = text
                        .char_indices()
                        .nth(500)
                        .map(|(i, _)| i)
                        .unwrap_or(text.len());
                    text[..end].to_string()
                } else {
                    text
                };

                results.push(Hit {
                    session_id: session_id.clone(),
                    timestamp: entry.timestamp.clone(),
                    source: source.to_string(),
                    text: truncated,
                });

                if results.len() >= limit {
                    break;
                }
            }
            if results.len() >= limit {
                break;
            }
        }

        serde_json::to_string_pretty(&results)
            .unwrap_or_else(|e| format!("JSON error: {e}"))
    }

    /// Get tool usage records from transcripts, optionally with output.
    #[tool(description = "Get tool usage history from transcripts. Shows which tools were called, their inputs, and optionally outputs. Filter by session or tool name.")]
    async fn get_transcript_tools(
        &self,
        Parameters(input): Parameters<GetTranscriptToolsInput>,
    ) -> String {
        let account_id = match self.get_account_id().await {
            Ok(id) => id,
            Err(e) => return e,
        };

        let limit = input.limit.unwrap_or(50).min(200) as usize;

        let s3_keys = match self.get_s3_keys(&account_id, input.session_id.as_deref(), 20).await {
            Ok(keys) => keys,
            Err(e) => return e,
        };

        let fetches = s3_keys
            .iter()
            .map(|key| fetch_transcript_bytes(&self.s3, &self.transcript_bucket, key));
        let all_bytes = join_all(fetches).await;

        #[derive(Serialize)]
        struct Record {
            session_id: String,
            timestamp: Option<String>,
            name: String,
            input: serde_json::Value,
            #[serde(skip_serializing_if = "Option::is_none")]
            output: Option<String>,
        }

        let mut records: Vec<Record> = Vec::new();

        for (i, maybe_bytes) in all_bytes.into_iter().enumerate() {
            let Some(bytes) = maybe_bytes else { continue };
            let entries = parse_entries(&bytes);
            let session_id = Self::session_id_from_key(&s3_keys[i], &account_id);

            let mut tool_output_map: HashMap<String, String> = HashMap::new();
            if input.include_output {
                for entry in &entries {
                    let Some(ref msg) = entry.message else {
                        continue;
                    };
                    if entry.r#type == "user" {
                        for result in extract_tool_results(&msg.content) {
                            tool_output_map.insert(result.tool_use_id, result.content);
                        }
                    }
                }
            }

            for entry in &entries {
                let Some(ref msg) = entry.message else {
                    continue;
                };
                let tool_uses = extract_tool_uses(&msg.content);
                for (id, name, tool_input) in tool_uses {
                    if let Some(ref filter) = input.tool_name {
                        if !name.eq_ignore_ascii_case(filter) {
                            continue;
                        }
                    }
                    let output = if input.include_output {
                        tool_output_map.get(&id).cloned()
                    } else {
                        None
                    };
                    records.push(Record {
                        session_id: session_id.clone(),
                        timestamp: entry.timestamp.clone(),
                        name,
                        input: tool_input,
                        output,
                    });
                    if records.len() >= limit {
                        break;
                    }
                }
                if records.len() >= limit {
                    break;
                }
            }
            if records.len() >= limit {
                break;
            }
        }

        serde_json::to_string_pretty(&serde_json::json!({
            "tool_uses": records,
            "total": records.len(),
        }))
        .unwrap_or_else(|e| format!("JSON error: {e}"))
    }

    /// Get parsed transcript for a specific session.
    #[tool(description = "Get parsed transcript for a session. Returns structured entries with text, tool uses, and optionally tool results.")]
    async fn get_transcript(
        &self,
        Parameters(input): Parameters<GetTranscriptInput>,
    ) -> String {
        let account_id = match self.get_account_id().await {
            Ok(id) => id,
            Err(e) => return e,
        };

        let s3_key = format!("{account_id}/{}.jsonl", input.session_id);
        let Some(bytes) =
            fetch_transcript_bytes(&self.s3, &self.transcript_bucket, &s3_key).await
        else {
            return "Transcript not found".to_string();
        };

        let entries = parse_entries(&bytes);

        #[derive(Serialize)]
        struct Entry {
            r#type: String,
            timestamp: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            text: Option<String>,
            tool_uses: Vec<ToolUse>,
            tool_results: Vec<ToolResult>,
        }

        #[derive(Serialize)]
        struct ToolUse {
            id: String,
            name: String,
            input: serde_json::Value,
        }

        #[derive(Serialize)]
        struct ToolResult {
            tool_use_id: String,
            content: String,
        }

        let mut results: Vec<Entry> = Vec::new();

        for entry in &entries {
            let Some(ref msg) = entry.message else {
                continue;
            };

            let text = extract_text(&msg.content);
            let text = if text.is_empty() { None } else { Some(text) };

            let tool_uses = extract_tool_uses(&msg.content)
                .into_iter()
                .map(|(id, name, input)| ToolUse { id, name, input })
                .collect();

            let tool_results = if input.include_tool_results {
                extract_tool_results(&msg.content)
                    .into_iter()
                    .map(|r| ToolResult {
                        tool_use_id: r.tool_use_id,
                        content: r.content,
                    })
                    .collect()
            } else {
                vec![]
            };

            results.push(Entry {
                r#type: entry.r#type.clone(),
                timestamp: entry.timestamp.clone(),
                text,
                tool_uses,
                tool_results,
            });
        }

        serde_json::to_string_pretty(&results)
            .unwrap_or_else(|e| format!("JSON error: {e}"))
    }
}

#[tool_handler]
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2025_03_26,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Claude Memory Cloud - search and retrieve past conversation memories, \
                 tool usage history, and find semantically similar content across all projects."
                    .to_string(),
            ),
        }
    }
}
