//! Claude Code JSONL transcript type definitions.
//!
//! Comprehensive Rust types that mirror the JSONL schema used by Claude Code
//! for conversation transcripts. See `SCHEMA.md` for the full analysis.
//!
//! Each line in a `.jsonl` file deserializes to [`TranscriptLine`].

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ============================================================
// Top-level line enum
// ============================================================

/// A single line in a Claude Code JSONL transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TranscriptLine {
    #[serde(rename = "user")]
    User(UserLine),

    #[serde(rename = "assistant")]
    Assistant(AssistantLine),

    #[serde(rename = "system")]
    System(SystemLine),

    #[serde(rename = "progress")]
    Progress(ProgressLine),

    #[serde(rename = "file-history-snapshot")]
    FileHistorySnapshot(FileHistorySnapshotLine),

    #[serde(rename = "custom-title")]
    CustomTitle(CustomTitleLine),

    #[serde(rename = "last-prompt")]
    LastPrompt(LastPromptLine),

    #[serde(rename = "agent-name")]
    AgentName(AgentNameLine),

    #[serde(rename = "pr-link")]
    PrLink(PrLinkLine),

    #[serde(rename = "queue-operation")]
    QueueOperation(QueueOperationLine),
}

// ============================================================
// Common fields (shared across user/assistant/system/progress)
// ============================================================

/// Fields shared by user, assistant, system, and progress lines.
///
/// Not extracted as a separate struct because each line type has
/// different optionality. Documented here for reference.
///
/// - `uuid`: Unique message ID
/// - `parentUuid`: Parent message ID (conversation tree)
/// - `sessionId`: Session UUID
/// - `timestamp`: ISO 8601
/// - `isSidechain`: Side conversation branch
/// - `cwd`: Working directory
/// - `gitBranch`: Git branch name
/// - `entrypoint`: Always "cli"
/// - `userType`: Always "external"
/// - `version`: Claude Code version string
/// - `slug`: Session slug
#[allow(dead_code)]
const _COMMON_FIELDS_DOC: () = ();

// ============================================================
// user
// ============================================================

/// A `user` type line — user input or tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserLine {
    pub uuid: String,
    pub timestamp: String,
    pub session_id: String,
    pub message: UserMessage,

    #[serde(default)]
    pub parent_uuid: Option<String>,
    #[serde(default)]
    pub is_sidechain: Option<bool>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub entrypoint: Option<String>,
    #[serde(default)]
    pub user_type: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,

    // user-specific fields
    #[serde(default)]
    pub prompt_id: Option<String>,
    #[serde(default)]
    pub permission_mode: Option<PermissionMode>,
    /// Polymorphic tool execution result. See SCHEMA.md for shapes.
    #[serde(default)]
    pub tool_use_result: Option<Value>,
    #[serde(default)]
    pub source_tool_assistant_uuid: Option<String>,
    #[serde(default, rename = "sourceToolUseID")]
    pub source_tool_use_id: Option<String>,
    #[serde(default)]
    pub is_compact_summary: Option<bool>,
    #[serde(default)]
    pub is_meta: Option<bool>,
    #[serde(default)]
    pub is_visible_in_transcript_only: Option<bool>,
    #[serde(default)]
    pub image_paste_ids: Option<Vec<i64>>,
    #[serde(default)]
    pub plan_content: Option<String>,
    #[serde(default)]
    pub todos: Option<Vec<Todo>>,
    #[serde(default)]
    pub forked_from: Option<ForkedFrom>,
    #[serde(default)]
    pub mcp_meta: Option<McpMeta>,
    #[serde(default)]
    pub agent_id: Option<String>,
}

/// The message payload inside a `user` line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub role: String,
    pub content: MessageContent,
}

/// User message content — either a plain string or an array of content blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    Default,
    AcceptEdits,
    Plan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Todo {
    pub content: String,
    pub status: String,
    #[serde(default)]
    pub active_form: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkedFrom {
    pub message_uuid: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpMeta {
    #[serde(default)]
    pub structured_content: Option<McpStructuredContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpStructuredContent {
    #[serde(default)]
    pub result: Option<String>,
}

// ============================================================
// assistant
// ============================================================

/// An `assistant` type line — AI response or tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantLine {
    pub uuid: String,
    pub timestamp: String,
    pub session_id: String,
    pub message: AssistantMessage,

    #[serde(default)]
    pub parent_uuid: Option<String>,
    #[serde(default)]
    pub is_sidechain: Option<bool>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub entrypoint: Option<String>,
    #[serde(default)]
    pub user_type: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,

    // assistant-specific fields
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub is_api_error_message: Option<bool>,
    #[serde(default)]
    pub api_error: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub forked_from: Option<ForkedFrom>,
}

/// The message payload inside an `assistant` line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub role: String,
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub stop_reason: Option<StopReason>,
    #[serde(default)]
    pub stop_sequence: Option<String>,
    /// Always "message" when present.
    #[serde(default, rename = "type")]
    pub message_type: Option<String>,
    #[serde(default)]
    pub usage: Option<Usage>,
    #[serde(default)]
    pub context_management: Option<Value>,
    #[serde(default)]
    pub container: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
}

/// Token usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: Option<i64>,
    #[serde(default)]
    pub output_tokens: Option<i64>,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<i64>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<i64>,
    #[serde(default)]
    pub cache_creation: Option<CacheCreation>,
    #[serde(default)]
    pub server_tool_use: Option<ServerToolUse>,
    #[serde(default)]
    pub service_tier: Option<String>,
    #[serde(default)]
    pub speed: Option<String>,
    #[serde(default)]
    pub inference_geo: Option<String>,
    #[serde(default)]
    pub iterations: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheCreation {
    #[serde(default)]
    pub ephemeral_5m_input_tokens: Option<i64>,
    #[serde(default)]
    pub ephemeral_1h_input_tokens: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerToolUse {
    #[serde(default)]
    pub web_search_requests: Option<i64>,
    #[serde(default)]
    pub web_fetch_requests: Option<i64>,
}

// ============================================================
// Content blocks (shared between user and assistant messages)
// ============================================================

/// A content block inside a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text {
        text: String,
    },

    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        #[serde(default)]
        signature: Option<String>,
    },

    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        /// Tool parameters — completely dynamic per tool.
        input: Value,
        #[serde(default)]
        caller: Option<Caller>,
    },

    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        #[serde(default)]
        content: Option<ToolResultContent>,
        #[serde(default)]
        is_error: Option<bool>,
    },

    #[serde(rename = "image")]
    Image {
        source: ImageSource,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Caller {
    #[serde(rename = "type")]
    pub caller_type: String,
}

/// Tool result content — either a plain string or an array of sub-blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    Text(String),
    Blocks(Vec<SubContentBlock>),
}

/// Sub-content block inside tool_result.content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SubContentBlock {
    #[serde(rename = "text")]
    Text { text: String },

    #[serde(rename = "image")]
    Image { source: ImageSource },

    #[serde(rename = "tool_reference")]
    ToolReference { tool_name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub media_type: Option<String>,
}

// ============================================================
// system
// ============================================================

/// A `system` type line — turn duration, hook results, API errors, compaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemLine {
    pub uuid: String,
    pub timestamp: String,
    pub session_id: String,
    pub subtype: SystemSubtype,

    #[serde(default)]
    pub parent_uuid: Option<String>,
    #[serde(default)]
    pub is_sidechain: Option<bool>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub entrypoint: Option<String>,
    #[serde(default)]
    pub user_type: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,

    // system-specific fields
    #[serde(default)]
    pub level: Option<SystemLevel>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub logical_parent_uuid: Option<String>,
    #[serde(default, rename = "toolUseID")]
    pub tool_use_id: Option<String>,
    #[serde(default)]
    pub forked_from: Option<ForkedFrom>,

    // turn_duration fields
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub message_count: Option<i64>,
    #[serde(default)]
    pub is_meta: Option<bool>,

    // stop_hook_summary fields
    #[serde(default)]
    pub hook_count: Option<i64>,
    #[serde(default)]
    pub hook_infos: Option<Vec<HookInfo>>,
    #[serde(default)]
    pub hook_errors: Option<Vec<String>>,
    #[serde(default)]
    pub prevented_continuation: Option<bool>,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub has_output: Option<bool>,

    // api_error fields
    #[serde(default)]
    pub error: Option<Value>,
    #[serde(default)]
    pub cause: Option<Value>,
    #[serde(default)]
    pub retry_attempt: Option<i64>,
    #[serde(default)]
    pub retry_in_ms: Option<f64>,
    #[serde(default)]
    pub max_retries: Option<i64>,

    // compact_boundary fields
    #[serde(default)]
    pub compact_metadata: Option<CompactMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemSubtype {
    TurnDuration,
    StopHookSummary,
    ApiError,
    CompactBoundary,
    LocalCommand,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemLevel {
    Info,
    Error,
    Suggestion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookInfo {
    pub command: String,
    #[serde(default)]
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactMetadata {
    #[serde(default)]
    pub pre_tokens: Option<i64>,
    #[serde(default)]
    pub trigger: Option<String>,
}

// ============================================================
// progress
// ============================================================

/// A `progress` type line — tool execution progress (ephemeral).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressLine {
    pub uuid: String,
    pub timestamp: String,
    pub session_id: String,

    #[serde(default)]
    pub parent_uuid: Option<String>,
    #[serde(default)]
    pub is_sidechain: Option<bool>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub entrypoint: Option<String>,
    #[serde(default)]
    pub user_type: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,

    // progress-specific fields
    #[serde(default, rename = "toolUseID")]
    pub tool_use_id: Option<String>,
    #[serde(default, rename = "parentToolUseID")]
    pub parent_tool_use_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub forked_from: Option<ForkedFrom>,
    /// Progress data — polymorphic by `data.type`.
    ///
    /// `data.type` values: `agent_progress`, `bash_progress`, `hook_progress`,
    /// `mcp_progress`, `query_update`, `search_results_received`, `waiting_for_task`
    #[serde(default)]
    pub data: Option<Value>,
}

// ============================================================
// file-history-snapshot
// ============================================================

/// A `file-history-snapshot` line — file backup tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHistorySnapshotLine {
    pub message_id: String,
    #[serde(default)]
    pub is_snapshot_update: Option<bool>,
    pub snapshot: Snapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub timestamp: Option<String>,
    /// Map of file path → backup info.
    #[serde(default)]
    pub tracked_file_backups: HashMap<String, FileBackup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileBackup {
    #[serde(default)]
    pub backup_file_name: Option<String>,
    #[serde(default)]
    pub backup_time: Option<String>,
    #[serde(default)]
    pub version: Option<i64>,
}

// ============================================================
// Metadata-only lines
// ============================================================

/// A `custom-title` line — user-set session title.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomTitleLine {
    pub custom_title: String,
    pub session_id: String,
}

/// A `last-prompt` line.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastPromptLine {
    pub last_prompt: String,
    pub session_id: String,
}

/// An `agent-name` line — subagent name record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentNameLine {
    pub agent_name: String,
    pub session_id: String,
}

/// A `pr-link` line — associated pull request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrLinkLine {
    pub pr_number: i64,
    pub pr_repository: String,
    pub pr_url: String,
    pub session_id: String,
    pub timestamp: String,
}

/// A `queue-operation` line.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueOperationLine {
    #[serde(default)]
    pub content: Option<String>,
    pub operation: String,
    pub session_id: String,
    pub timestamp: String,
}

// ============================================================
// Subagent meta (separate .meta.json file)
// ============================================================

/// Content of `agent-{id}.meta.json` files.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentMeta {
    pub agent_type: String,
    pub description: String,
}
