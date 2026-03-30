---
name: recall
description: 過去の会話・セッション記憶をセマンティック検索。「覚えてる？」「前に話した」「以前やった」「思い出せる？」「話してた」「調べた」など、ユーザーが過去の会話を参照している時に自動で使う。
user-invocable: true
---

# /recall - 過去の会話を検索

チームの過去の Claude Code セッションからセマンティック検索を行います。
ユーザーが過去の会話内容について質問した場合（「〜覚えてる？」「前に〜話したよね」「以前〜やった」等）、このスキルを自動的に使用してください。

## 使い方

### `/recall <query>`

過去の会話から関連するセッションを検索します。

```bash
memory-cloud recall "<query>"
```

### ユーザーを絞り込む

`--user` オプションで検索対象のユーザーを絞り込めます:

```bash
memory-cloud recall "<query>" --user me              # 自分のみ
memory-cloud recall "<query>" --user foo@example.com  # 特定ユーザー (email)
memory-cloud recall "<query>" --user <user_id>        # 特定ユーザー (ID)
```

省略時は自分 + 共有を受けた全ユーザーが検索対象です。

### パラメータ

- `query` — 検索クエリ（必須）
- `--user` — 検索対象ユーザー（省略可）
- `-k` / `--top-k` — 結果件数（デフォルト: 5）

レスポンスには `session_id`, `score`, `text` に加えて `metadata` (`user_id`, `project`, `created_at` 等) が含まれます。
`session_id` を使って `/transcript get` で全文を取得できます。

## 例

- `/recall CDKデプロイでエラー` — デプロイ関連の過去の会話を検索
- `/recall 認証フローの実装 --user me` — 自分の認証に関する過去の議論を検索
