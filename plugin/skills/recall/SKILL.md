---
name: recall
description: 過去のセッション記憶をセマンティック検索
user-invocable: true
---

# /recall - 過去の会話を検索

チームの過去の Claude Code セッションからセマンティック検索を行います。

## 使い方

### `/recall <query>`

過去の会話から関連するセッションを検索します。

```bash
memory-cloud recall "<query>"
```

パラメータ:
- `query` — 検索クエリ（必須）

## 例

- `/recall CDKデプロイでエラー` — デプロイ関連の過去の会話を検索
- `/recall 認証フローの実装` — 認証に関する過去の議論を検索
