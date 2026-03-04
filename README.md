# claude-memory-cloud

Claude Code の会話記憶をクラウドに保存・検索するバックエンド。

## アーキテクチャ

- **API Lambda** (Axum) — Hook からのデータ書き込み + セッション読み取り
- **MCP Lambda** — Claude Code からの検索・取得（OAuth 認証）
- **OAuth Proxy Lambda** — Cognito OAuth2 フロー
- **Aurora Serverless v2** (PostgreSQL + pgvector) — turns / sessions / transcripts
- **S3** — transcript JSONL の生データ保存
- **Bedrock Titan Embed v2** — ベクトル検索用 embedding 生成

## API エンドポイント

| メソッド | パス | 用途 |
|---------|------|------|
| GET | `/api/sessions` | セッション一覧 |
| GET | `/api/sessions/{sid}/turns` | セッション内ターン取得 |
| DELETE | `/api/sessions/{sid}/turns` | セッション内ターン削除 |
| POST | `/api/turns/batch` | ターン一括 upsert |
| POST | `/api/transcripts` | transcript presigned URL 取得 |
| GET | `/api/health` | ヘルスチェック |

## MCP ツール

| ツール | 用途 |
|--------|------|
| `search_memory` | FTS + ベクトルのハイブリッド検索 |
| `find_similar` | ベクトル類似検索 |
| `get_sessions` | セッション一覧 |
| `get_session_turns` | ターン取得 |
| `search_transcripts` | transcript テキスト検索 |
| `get_transcript` | transcript 全文取得 |
| `get_transcript_tools` | ツール使用履歴 |

## Crate 構成

| Crate | 役割 |
|-------|------|
| `api` | API Lambda ハンドラー |
| `mcp-server` | MCP サーバー Lambda |
| `oauth-proxy` | OAuth2 プロキシ Lambda |
| `oauth-metadata` | OAuth メタデータ配信 Lambda |
| `common` | 共有モデル・認証・embedding・transcript パーサー |

## プラグイン

クライアント側のプラグインは別リポジトリ: [memory-cloud-plugin](https://github.com/masuibass/memory-cloud-plugin)
