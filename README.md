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
  ├── S3: transcript 保存・取得 (presigned URL)
  └── Bedrock KB: セマンティック検索
        └── S3 Vectors: embedding 格納
```

VPC なし。DB なし。Lambda 1 つ。

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

## プラグイン

Claude Code プラグインとして以下を提供：

- **Stop hook** — セッション終了時に transcript を自動アップロード
- **`/memory-cloud:recall`** — 過去の会話をセマンティック検索
- **`/memory-cloud:transcript`** — transcript の手動アップロード・ダウンロード

## CLI コマンド

| コマンド | 用途 |
|---------|------|
| `memory-cloud init <url>` | API URL 設定 |
| `memory-cloud login` | Cognito 認証 |
| `memory-cloud recall <query>` | セマンティック検索 |
| `memory-cloud transcript put <file>` | transcript アップロード |
| `memory-cloud transcript get <sid>` | transcript ダウンロード |
| `memory-cloud sessions list` | セッション一覧 |

## 自分の環境にデプロイ

### 前提条件

- AWS アカウント
- Node.js, Rust, [cargo-lambda](https://www.cargo-lambda.info/)

### デプロイ

```bash
cargo lambda build --release --arm64
cd cdk && npm install && npx cdk deploy
```

デプロイ後、出力される API URL を `memory-cloud init` に渡す。

## API エンドポイント

| メソッド | パス | 認証 | 用途 |
|---------|------|------|------|
| GET | `/config` | なし | Cognito 情報 |
| POST | `/transcript` | JWT | presigned URL 発行 |
| GET | `/transcript/{uid}/{sid}` | JWT | transcript 取得 |
| DELETE | `/transcript/{uid}/{sid}` | JWT | transcript 削除 |
| GET | `/sessions` | JWT | セッション一覧 |
| POST | `/recall` | JWT | KB 検索 |

## AWS リソース

| サービス | 用途 |
|---------|------|
| Cognito User Pool | PKCE 認証 |
| API Gateway HTTP API | JWT 認証 + ルーティング |
| Lambda | API (Axum / Rust) |
| S3 | transcript 保存 |
| S3 Vectors | embedding 格納 |
| Bedrock Knowledge Base | 自動 embedding + Retrieve API |
