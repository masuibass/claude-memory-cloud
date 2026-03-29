#!/bin/bash
set -euo pipefail

# 1. CLI バイナリの存在チェック
if ! command -v memory-cloud >/dev/null 2>&1; then
  echo "[memory-cloud] CLI がインストールされていません。"
  echo "  インストール: https://github.com/masuibass/claude-memory-cloud/releases"
  echo "  ダウンロード後: tar xzf memory-cloud-<arch>.tar.gz && mv memory-cloud ~/.local/bin/"
  exit 0
fi

# 2. init 済みチェック (config ファイルの存在)
# macOS: ~/Library/Application Support/memory-cloud
# Linux: ~/.config/memory-cloud
if [ "$(uname)" = "Darwin" ]; then
  CONFIG_DIR="${HOME}/Library/Application Support/memory-cloud"
else
  CONFIG_DIR="${HOME}/.config/memory-cloud"
fi
if [ ! -f "${CONFIG_DIR}/config.toml" ]; then
  echo "[memory-cloud] 初期設定が必要です。"
  echo "  memory-cloud init <API_URL>"
  echo "  memory-cloud login"
  exit 0
fi

# 3. login 済みチェック (tokens ファイルの存在)
if [ ! -f "${CONFIG_DIR}/tokens.json" ]; then
  echo "[memory-cloud] ログインが必要です。"
  echo "  memory-cloud login"
  exit 0
fi
