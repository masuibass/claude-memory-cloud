# claude-memory-cloud

Claude Code の会話をクラウドに保存し、セマンティック検索で再利用する。

## アーキテクチャ

```
Claude Code
  ↓ Stop hook (transcript 自動アップロード)
CLI (memory-cloud)
  ↓ PKCE auth + JWT
API Gateway HTTP API
  ↓ Cognito JWT Authorizer
Lambda (Axum)
  ├── S3 (raw): JSONL 保存 (presigned URL)
  ├── DynamoDB: 共有管理
  └── Bedrock KB: セマンティック検索

S3 (raw) → SQS → Parser Lambda → S3 (parsed) → KB Sync Lambda → Bedrock KB
                   JSONL→Markdown       .md + .metadata.json       自動 ingestion

削除パイプライン:
S3 (raw) 削除 → SQS → Parser Lambda → S3 (parsed) 削除 → KB Sync Lambda → KB 更新
```

VPC なし。RDB なし。サーバーレス。

## クイックスタート

### 1. プラグインインストール

```
/plugin marketplace add masuibass/claude-memory-cloud
/plugin install memory-cloud@claude-memory-cloud
```

### 2. CLI インストール

[GitHub Releases](https://github.com/masuibass/claude-memory-cloud/releases) からバイナリをダウンロード：

```bash
tar xzf memory-cloud-<arch>.tar.gz
mv memory-cloud ~/.local/bin/
```

### 3. CLI 初期設定

```bash
memory-cloud init <API_URL>
memory-cloud login
```

### 4. 動作確認

```bash
memory-cloud recall "S3 presigned URL"
```

### 5. 過去の会話を一括アップロード

```bash
memory-cloud transcript bulk-upload
```

`~/.claude/projects` 以下の全 JSONL を一括アップロードします。

## プラグイン

Claude Code プラグインとして以下を提供：

- **Stop hook** — セッション終了時に transcript を自動アップロード
- **`/memory-cloud:recall`** — 過去の会話をセマンティック検索
- **`/memory-cloud:transcript`** — transcript の手動アップロード・ダウンロード・削除
- **`/memory-cloud:shares`** — 共有管理

## CLI コマンド

| コマンド | 用途 |
|---------|------|
| `memory-cloud init <url>` | API URL 設定 |
| `memory-cloud login` | Cognito 認証 |
| `memory-cloud whoami` | 自分の user ID を表示 |
| `memory-cloud recall <query>` | セマンティック検索 (`--user` でユーザー絞り込み) |
| `memory-cloud transcript put <file>` | transcript アップロード |
| `memory-cloud transcript get <sid>` | transcript ダウンロード (共有相手のも取得可) |
| `memory-cloud transcript purge` | 自分の全 transcript を削除 |
| `memory-cloud transcript bulk-upload` | 一括アップロード |
| `memory-cloud sessions list` | セッション一覧 |
| `memory-cloud shares add <id_or_email>` | 相手にトランスクリプトを共有 |
| `memory-cloud shares revoke <user_id>` | 共有を取り消し |
| `memory-cloud shares remove <user_id>` | 受けた共有を解除 |
| `memory-cloud shares list` | 共有一覧 (email 付き) |

## 共有

ユーザー間でトランスクリプトの検索権限を共有できます。

```bash
# 自分のトランスクリプトを共有 (email でも user ID でも指定可)
memory-cloud shares add foo@example.com

# 共有を取り消し
memory-cloud shares revoke <user_id>
```

`recall` 時は自分のトランスクリプト + 共有されたトランスクリプトが検索対象になります（`user_id IN [...]` フィルタで強制）。
`--user` オプションで検索対象を絞り込めます (`me`, email, user ID)。

`transcript get` は共有相手のトランスクリプトも session ID のみで取得できます。

## 自分の環境にデプロイ

### 前提条件

- AWS アカウント
- Node.js, Rust, [cargo-lambda](https://www.cargo-lambda.info/)

### デプロイ

```bash
cargo lambda build --release --arm64 -p api -p parser
cd cdk && npm install && npx cdk deploy
```

デプロイ後、出力される API URL を `memory-cloud init` に渡す。

## API エンドポイント

| メソッド | パス | 認証 | 用途 |
|---------|------|------|------|
| GET | `/config` | なし | Cognito 情報 + バージョン |
| GET | `/whoami` | JWT | 自分の user ID |
| POST | `/transcript` | JWT | presigned URL 発行 |
| GET | `/transcript/{sid}` | JWT | transcript 取得 (共有相手も検索) |
| DELETE | `/transcript/{sid}` | JWT | transcript 削除 |
| DELETE | `/transcripts` | JWT | 全 transcript 削除 |
| GET | `/sessions` | JWT | セッション一覧 |
| POST | `/recall` | JWT | KB 検索 (`user` パラメータで絞り込み可) |
| GET | `/shares` | JWT | 共有一覧 (email 付き) |
| POST | `/shares` | JWT | 共有作成 (email or user ID) |
| DELETE | `/shares/{owner_id}` | JWT | 受けた共有を解除 |
| DELETE | `/shares/recipients/{recipient_id}` | JWT | 共有取り消し |

## AWS リソース

| サービス | 用途 |
|---------|------|
| Cognito User Pool | PKCE 認証 |
| API Gateway HTTP API | JWT 認証 + ルーティング |
| Lambda (API) | REST API (Axum / Rust) |
| Lambda (Parser) | JSONL → Markdown パース + 削除時の parsed クリーンアップ |
| Lambda (KB Sync) | parsed S3 作成/削除 → KB ingestion 自動トリガー |
| S3 (raw) | JSONL 保存 |
| S3 (parsed) | Markdown + metadata.json |
| S3 Vectors | embedding 格納 |
| SQS | raw → parser バッファリング + DLQ |
| DynamoDB | 共有管理 |
| Bedrock Knowledge Base | セマンティックチャンキング + Retrieve API |
