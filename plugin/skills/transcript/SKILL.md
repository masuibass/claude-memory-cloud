---
name: transcript
description: transcript のアップロード・ダウンロード・一括アップロード
user-invocable: true
---

# /transcript - Transcript 操作

Claude Code セッションの transcript を管理します。

## 使い方

### `/transcript put <file>`

transcript ファイルをアップロードします。

```bash
memory-cloud transcript put <file>
```

プロジェクトはファイルパスから自動推定されます。明示指定も可能:

```bash
memory-cloud transcript put <file> --project <project_hash>
```

### `/transcript get <session_id>`

transcript をダウンロードして表示します。デフォルトは parsed (Markdown) 形式です。
共有を受けた他ユーザーの transcript も取得できます。

```bash
memory-cloud transcript get <session_id>
```

生の JSONL が必要な場合は `--raw` を付けます:

```bash
memory-cloud transcript get <session_id> --raw
```

### `/transcript bulk-upload`

`~/.claude/projects` 以下の全 JSONL を一括アップロードします。

```bash
memory-cloud transcript bulk-upload
```

## 備考

- stop hook により、セッション終了時に自動アップロードされます
- 手動アップロードは過去のセッションを追加したい場合に使います
- bulk-upload は初回セットアップ時にまとめてアップロードするのに便利です
