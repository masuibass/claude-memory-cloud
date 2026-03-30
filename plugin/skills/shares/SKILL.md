---
name: shares
description: トランスクリプトの共有管理
user-invocable: true
---

# /shares - 共有管理

ユーザー間でトランスクリプトの検索権限を管理します。

**重要**: コマンドはユーザーの作業ディレクトリで実行してください。このスキルのディレクトリに cd しないでください。

## 使い方

### `/shares add <recipient>`

自分のトランスクリプトを相手に共有します。user ID でも email でも指定できます。

```bash
memory-cloud shares add foo@example.com
memory-cloud shares add <user_id>
```

### `/shares revoke <recipient_id>`

自分が出した共有を取り消します。

```bash
memory-cloud shares revoke <user_id>
```

### `/shares remove <owner_id>`

受けた共有を解除します（相手のトランスクリプトが検索対象から外れる）。

```bash
memory-cloud shares remove <user_id>
```

### `/shares list`

共有一覧を表示します。各ユーザーの email も表示されます。

```bash
memory-cloud shares list
```

## 備考

- recall 時は自分 + 共有元のトランスクリプトが検索対象になります
- transcript get は共有元のトランスクリプトも取得できます
- 共有は片方向です（相互に共有したい場合は双方で add してください）
