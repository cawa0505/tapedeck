# ref：wtype CLI 語法（Wayland 鍵盤注入）

> 來源：GitHub atx/wtype README（2026-08-08 抓取）。供 OQ-02（src/engine/input.rs）實作參考。

## 基本用法

| 目的 | 指令 |
|------|------|
| 輸入 unicode 文字 | `wtype ∇⋅∇ψ = ρ`（參數即文字） |
| keystroke 間延遲 | `wtype foo -d 120 bar`（`-d` 影響其後 keystroke，單位 ms） |
| 從 stdin 輸入 | `echo everything \| wtype -d 12 -`（`-` = 讀 stdin） |

## Modifier（組合鍵）

- `-M <mod>`：按下 modifier（如 `-M ctrl`）
- `-m <mod>`：放開 modifier
- 範例：Ctrl+C = `wtype -M ctrl c -m ctrl`
- 多 modifier：重複 `-M`/`-m`（如 `-M ctrl -M shift`）

## 具名按鍵（xkb_keysym）

- `-P <key>`：按下具名按鍵（名字依 xkb_keysym_get_name，如 `left`、`Return`）
- `-p <key>`：放開具名按鍵
- 範例：按放 Left = `wtype -P left -p left`
- `-s <ms>`：在 key event 串流中插入延遲
- 範例：按住 Right 1000ms = `wtype -P right -s 1000 -p right`

## 行為

- wtype 結束時會釋放所有仍按住的鍵/modifier（compositor 銷毀 virtual keyboard object）
- 依 xkbcommon keysym 命名（xkb_keysym_get_name）

## 對 OQ-02 的映射（src/engine/input.rs）

| ScriptCommand | wtype CLI |
|---------------|-----------|
| `Type(text)` | `wtype <text>` |
| `Key(name, n)` | `-P name -p name` 重複 n 次（`-s` 隔開） |
| `Shortcut(combo)` | `-M mod1 ... key ... -m mod1 ...` |

## 來源
- https://github.com/atx/wtype/blob/master/README.md
