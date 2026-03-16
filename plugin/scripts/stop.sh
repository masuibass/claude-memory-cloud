#!/bin/bash
set -euo pipefail

# Read hook input from stdin
HOOK_INPUT=$(cat)
TRANSCRIPT_PATH=$(echo "$HOOK_INPUT" | jq -r '.transcript_path // empty')
SESSION_ID=$(echo "$HOOK_INPUT" | jq -r '.session_id // empty')

if [ -z "$TRANSCRIPT_PATH" ] || [ -z "$SESSION_ID" ]; then
  exit 0
fi

if [ ! -f "$TRANSCRIPT_PATH" ]; then
  exit 0
fi

# Upload transcript
memory-cloud transcript put "$TRANSCRIPT_PATH" 2>/dev/null || true
