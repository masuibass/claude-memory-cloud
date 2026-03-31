# memory-cloud

Rust workspace + CDK (TypeScript) のモノレポ。

## ビルド・テスト

```bash
cargo check                                          # 全 crate の型チェック
cargo test                                           # 全テスト
cargo lambda build --release --arm64 -p api -p parser  # Lambda ビルド (ARM64)
cargo build --release -p cli                          # CLI ビルド
cd cdk && pnpm install && pnpm cdk synth              # CDK
```

## crate 構成

| crate | 役割 |
|-------|------|
| cli | `memory-cloud` CLI バイナリ |
| api | Lambda ハンドラ (API Gateway) |
| parser | transcript JSONL パーサー |
| transcript | transcript 型定義・ユーティリティ |

## Plugin (`plugin/`)

Claude Code プラグインとして配布。skills / hooks / scripts を含む。
変更時は利用者側への影響を考慮すること。

## コード規約

- `cargo clippy` 警告ゼロを維持
- 依存追加は workspace.dependencies に集約
