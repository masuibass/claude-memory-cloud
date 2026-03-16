---
name: transcript
description: transcript のアップロード・ダウンロード
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

### `/transcript get <session_id>`

transcript をダウンロードして表示します。

```bash
memory-cloud transcript get <session_id>
```

## 備考

- stop hook により、セッション終了時に自動アップロードされます
- 手動アップロードは過去のセッションを追加したい場合に使います
