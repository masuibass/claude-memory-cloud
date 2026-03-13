# claude-memory-cloud

チームの Claude Code 会話をクラウドに保存し、セマンティック検索で再利用する。

## アーキテクチャ

```
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

## セットアップ

### デプロイ

```bash
# CDK
cd cdk && npm install
npx cdk deploy

# API Lambda ビルド
cargo lambda build --release --arm64
```

### CLI インストール

[GitHub Releases](https://github.com/masuibass/claude-memory-cloud/releases) からバイナリをダウンロード、または：

```bash
cargo install --path crates/cli
```

### CLI 初期設定

```bash
memory-cloud init <API_URL>   # /config から Cognito 情報を取得
memory-cloud login             # ブラウザで Cognito 認証 (PKCE)
```

## CLI コマンド

| コマンド | 用途 |
|---------|------|
| `memory-cloud init <url>` | API URL 設定 |
| `memory-cloud login` | Cognito 認証 |
| `memory-cloud recall <query>` | セマンティック検索 |
| `memory-cloud transcript put <file>` | transcript アップロード |
| `memory-cloud transcript get <sid>` | transcript ダウンロード |
| `memory-cloud sessions list` | セッション一覧 |

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

## Crate 構成

| Crate | 役割 |
|-------|------|
| `api` | API Lambda |
| `cli` | CLI バイナリ (`memory-cloud`) |
