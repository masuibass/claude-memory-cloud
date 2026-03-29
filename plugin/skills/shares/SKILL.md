---
name: shares
description: トランスクリプトの共有管理
user-invocable: true
---

# /shares - 共有管理

ユーザー間でトランスクリプトの検索権限を管理します。

## 使い方

### `/shares add <user_id>`

自分のトランスクリプトを相手に共有します。

```bash
memory-cloud shares add <Cognito sub>
```

### `/shares revoke <user_id>`

自分が出した共有を取り消します。

```bash
memory-cloud shares revoke <Cognito sub>
```

### `/shares remove <user_id>`

受けた共有を解除します（相手のトランスクリプトが検索対象から外れる）。

```bash
memory-cloud shares remove <Cognito sub>
```

### `/shares list`

共有一覧を表示します。

```bash
memory-cloud shares list
```

## 備考

- recall 時は自分 + 共有元のトランスクリプトのみが検索対象になります
- 共有は片方向です（相互に共有したい場合は双方で add してください）
