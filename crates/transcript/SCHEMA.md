# Claude Code JSONL Transcript Schema

Analysis of 871 files / 83,589 lines across all local projects.

## Directory Structure

```
~/.claude/projects/
  {project-hash}/                            # e.g. -Users-masui-mutsuo-projects-foo
    {session-uuid}.jsonl                     # Main conversation log
    {session-uuid}/                          # Per-session auxiliary data
      subagents/
        agent-{id}.jsonl                     # Subagent conversation (same JSONL format)
        agent-{id}.meta.json                 # {"agentType": "...", "description": "..."}
      tool-results/
        {tool-use-id-or-hash}.txt            # Large tool output offloaded from JSONL
    memory/                                  # Auto-memory (not part of transcript)
```

- `{project-hash}` is the project's absolute path with `/` replaced by `-`, prefixed with `-`.
  Example: `/Users/masui.mutsuo/projects/foo` → `-Users-masui-mutsuo-projects-foo`

## Line Types

Each JSONL line is a JSON object with a `type` field discriminating the variant.

| type                    | Description                              | Count |
|-------------------------|------------------------------------------|-------|
| `user`                  | User input or tool result                | High  |
| `assistant`             | AI response or tool call                 | High  |
| `system`                | Turn duration, hooks, errors, compaction | Med   |
| `progress`              | Tool execution progress (ephemeral)      | Med   |
| `file-history-snapshot` | File backup tracking                     | Low   |
| `custom-title`          | User-set session title                   | Low   |
| `last-prompt`           | Last prompt text                         | Low   |
| `agent-name`            | Subagent name                            | Low   |
| `pr-link`               | Associated PR                            | Low   |
| `queue-operation`       | Queue operation log                      | Low   |

## Common Fields (present on user/assistant/system/progress)

| Field         | Type    | Description                                |
|---------------|---------|--------------------------------------------|
| `uuid`        | string  | Unique message ID                          |
| `parentUuid`  | string? | Parent message ID (conversation tree)      |
| `sessionId`   | string  | Session UUID                               |
| `timestamp`   | string  | ISO 8601 timestamp                         |
| `isSidechain` | bool    | Whether this is a side conversation branch |
| `cwd`         | string  | Working directory at time of message       |
| `gitBranch`   | string? | Git branch name                            |
| `entrypoint`  | string  | Always "cli"                               |
| `userType`    | string  | Always "external"                          |
| `version`     | string  | Claude Code version                        |
| `slug`        | string? | Session slug                               |

## `user` Line

### Top-level fields

| Field                    | Type                    | Description                          |
|--------------------------|-------------------------|--------------------------------------|
| `message`                | Message                 | The user message                     |
| `promptId`               | string?                 | Prompt identifier                    |
| `permissionMode`         | string?                 | "default" \| "acceptEdits" \| "plan" |
| `toolUseResult`          | Value?                  | Tool execution result (polymorphic)  |
| `sourceToolAssistantUUID`| string?                 | UUID of assistant that invoked tool  |
| `sourceToolUseID`        | string?                 | Tool use ID this result responds to  |
| `isCompactSummary`       | bool?                   | Compacted summary message            |
| `isMeta`                 | bool?                   | Meta-message flag                    |
| `isVisibleInTranscriptOnly` | bool?                | Visible only in transcript           |
| `imagePasteIds`          | int[]?                  | Pasted image IDs                     |
| `planContent`            | string?                 | Plan mode content                    |
| `todos`                  | Todo[]?                 | Task list state                      |
| `forkedFrom`             | ForkedFrom?             | Fork origin                          |
| `mcpMeta`                | McpMeta?                | MCP structured content               |
| `agentId`                | string?                 | Subagent ID                          |

### Message (user)

| Field     | Type                        | Description                     |
|-----------|-----------------------------|---------------------------------|
| `role`    | "user"                      |                                 |
| `content` | string \| ContentBlock[]    | Plain text or structured blocks |

### ContentBlock variants (user)

- **text**: `{type: "text", text: string}`
- **tool_result**: `{type: "tool_result", tool_use_id: string, content?: string | SubContentBlock[], is_error?: bool}`
- **image**: `{type: "image", source: {type: "base64", data: string, media_type: string}}`

### SubContentBlock (inside tool_result.content)

- **text**: `{type: "text", text: string}`
- **image**: `{type: "image", source: {type: "base64", data: string, media_type: string}}`
- **tool_reference**: `{type: "tool_reference", tool_name: string}`

### toolUseResult (polymorphic — serde_json::Value)

This field varies per tool. Common shapes:

- **string**: Plain text result
- **array**: `[{type: "text", text: "..."}]`
- **Bash result**: `{stdout, stderr, code, interrupted, durationMs, durationSeconds, outputFile?, persistedOutputPath?, ...}`
- **Read result**: `{file: {content, filePath, numLines, totalLines, startLine, type?, base64?, dimensions?, originalSize?}, ...}`
- **Edit result**: `{filePath, oldString, newString, structuredPatch: [{oldStart, oldLines, newStart, newLines, lines}], ...}`
- **Glob result**: `{filenames: string[], numFiles, matches: string[], ...}`
- **Grep result**: `{content, numMatches, numLines, ...}`
- **Agent result**: `{isAgent, totalTokens, totalToolUseCount, usage, tokenSaverOutput?, ...}`
- **Write result**: `{filePath, ...}`
- **AskUserQuestion result**: `{questions, answers, annotations, ...}`
- **Task result**: `{task: {id, status, description, subject, ...}, taskId, tasks?, ...}`
- **WebSearch result**: `{results: [{content: [{title, url}], ...}], ...}`
- **ToolSearch result**: `{results, total_deferred_tools, ...}`
- **Skill result**: `{commandName, ...}`

## `assistant` Line

### Top-level fields

| Field              | Type    | Description                         |
|--------------------|---------|-------------------------------------|
| `message`          | Message | The assistant message               |
| `requestId`        | string? | API request ID                      |
| `agentId`          | string? | Subagent ID                         |
| `isApiErrorMessage`| bool?   | Whether this is an API error        |
| `apiError`         | string? | API error text                      |
| `error`            | string? | Error text                          |
| `forkedFrom`       | ForkedFrom? | Fork origin                     |

### Message (assistant)

| Field               | Type            | Description                                     |
|---------------------|-----------------|-------------------------------------------------|
| `role`              | "assistant"     |                                                 |
| `model`             | string          | Model ID (see below)                            |
| `id`                | string          | API message ID                                  |
| `content`           | ContentBlock[]  | Response content blocks                         |
| `stop_reason`       | string?         | "end_turn" \| "tool_use" \| "max_tokens" \| "stop_sequence" |
| `stop_sequence`     | string?         | Stop sequence if applicable                     |
| `type`              | "message"       | Always "message"                                |
| `usage`             | Usage           | Token usage                                     |
| `context_management`| object?         | Context management info                         |
| `container`         | null?           | Container info                                  |

### Observed models

`claude-opus-4-6`, `claude-opus-4-5-20251101`, `claude-sonnet-4-6`, `claude-sonnet-4-5-20250929`, `claude-haiku-4-5-20251001`, `<synthetic>`

### ContentBlock variants (assistant)

- **text**: `{type: "text", text: string}`
- **thinking**: `{type: "thinking", thinking: string, signature?: string}`
- **tool_use**: `{type: "tool_use", id: string, name: string, input: object, caller?: {type: "direct"}}`

### Usage

| Field                          | Type    |
|--------------------------------|---------|
| `input_tokens`                 | int     |
| `output_tokens`                | int     |
| `cache_creation_input_tokens`  | int     |
| `cache_read_input_tokens`      | int     |
| `cache_creation`               | object? |
| `cache_creation.ephemeral_5m_input_tokens`  | int? |
| `cache_creation.ephemeral_1h_input_tokens`  | int? |
| `server_tool_use`              | object? |
| `server_tool_use.web_search_requests` | int? |
| `server_tool_use.web_fetch_requests`  | int? |
| `service_tier`                 | string? |
| `speed`                        | string? |
| `inference_geo`                | string? |
| `iterations`                   | array?  |

## `system` Line

### Top-level fields

| Field                  | Type         | Description                     |
|------------------------|--------------|---------------------------------|
| `subtype`              | string       | See subtypes below              |
| `level`                | string?      | "info" \| "error" \| "suggestion" |
| `content`              | string?      | Message content                 |
| `agentId`              | string?      | Subagent ID                     |
| `logicalParentUuid`    | string?      | Logical parent message          |
| `toolUseID`            | string?      | Related tool use                |
| `forkedFrom`           | ForkedFrom?  | Fork origin                     |

### Subtypes

- **turn_duration**: `{durationMs: int, messageCount: int, isMeta?: bool}`
- **stop_hook_summary**: `{hookCount: int, hookInfos: HookInfo[], hookErrors: string[], preventedContinuation: bool, stopReason: string, hasOutput: bool}`
- **api_error**: `{error: object, retryAttempt?: int, retryInMs?: float, maxRetries?: int}`
- **compact_boundary**: `{compactMetadata: {preTokens: int, trigger: string}, content?: string}`
- **local_command**: (no additional fields observed)

### HookInfo

| Field       | Type   |
|-------------|--------|
| `command`   | string |
| `durationMs`| int    |

## `progress` Line

| Field            | Type    | Description          |
|------------------|---------|----------------------|
| `toolUseID`      | string  | Related tool use     |
| `parentToolUseID`| string? | Parent tool use      |
| `agentId`        | string? | Subagent ID          |
| `data`           | object  | Progress data (polymorphic by data.type) |

### data.type values

`agent_progress`, `bash_progress`, `hook_progress`, `mcp_progress`, `query_update`, `search_results_received`, `waiting_for_task`

## `file-history-snapshot` Line

| Field              | Type                         |
|--------------------|------------------------------|
| `messageId`        | string                       |
| `isSnapshotUpdate` | bool                         |
| `snapshot`         | Snapshot                     |

### Snapshot

| Field                | Type                              |
|----------------------|-----------------------------------|
| `messageId`          | string                            |
| `timestamp`          | string                            |
| `trackedFileBackups` | Map<string, FileBackup>           |

### FileBackup

| Field           | Type    |
|-----------------|---------|
| `backupFileName`| string? |
| `backupTime`    | string  |
| `version`       | int     |

## Metadata-only Lines

### custom-title

| Field         | Type   |
|---------------|--------|
| `customTitle` | string |
| `sessionId`   | string |

### last-prompt

| Field        | Type   |
|--------------|--------|
| `lastPrompt` | string |
| `sessionId`  | string |

### agent-name

| Field       | Type   |
|-------------|--------|
| `agentName` | string |
| `sessionId` | string |

### pr-link

| Field          | Type   |
|----------------|--------|
| `prNumber`     | int    |
| `prRepository` | string |
| `prUrl`        | string |
| `sessionId`    | string |
| `timestamp`    | string |

### queue-operation

| Field       | Type   |
|-------------|--------|
| `content`   | string |
| `operation` | string |
| `sessionId` | string |
| `timestamp` | string |

## Subagent Meta (separate file: agent-{id}.meta.json)

```json
{"agentType": "Explore", "description": "Explore repo structure deeply"}
```

## Observed Tool Names (42)

### Built-in
Agent, AskUserQuestion, Bash, Edit, EnterPlanMode, ExitPlanMode, Glob, Grep, Read, Skill, Task, TaskCreate, TaskList, TaskOutput, TaskUpdate, TodoWrite, ToolSearch, WebFetch, WebSearch, Write

### MCP
mcp__aws-knowledge-mcp__aws___*, mcp__backlog-mcp__*, mcp__drawio__*, mcp__memory-cloud__*, mcp__plugin_memory-cloud_memory-cloud__*, mcp__plugin_serena_serena__*
