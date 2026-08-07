# vhs .tape 格式參考（2026-08-08 本機調研）

> 來源：本機 `vhs manual`（vhs version: 2026-08-08 man page）+ `vhs validate` 實測。
> 用途：roll-dsl 轉譯層（REQ-7）的語法對照基準。tapedeck 的 .roll 是「vhs 轉譯層 + tapedeck 自動化層」，因此 parser 必須能解析下列 vhs 全集指令並原樣透傳至 .tape。

## vhs 指令全集（manual 官方清單）

```
Output <path>.(gif|webm|mp4)
Require <program>
Set <setting> <value>
Sleep <time>
Type "<string>"
Ctrl [+Alt][+Shift]+<char>
Backspace [repeat]
Delete [repeat]
Insert [repeat]
Down [repeat]
Enter [repeat]
Left [repeat]
Right [repeat]
Tab [repeat]
Up [repeat]
PageUp [repeat]
PageDown [repeat]
ScrollUp [repeat]
ScrollDown [repeat]
Hide
Show
Wait[+Screen][@<timeout>] /<regexp>/
Escape
Alt+<key>
Space [repeat]
Source <path>.tape
Screenshot <path>.png
Copy "<string>"
Paste
```

## Set 設定選項（manual 官方清單）

```
Set Shell <string>
Set FontSize <number>
Set FontFamily <string>
Set Height <number>
Set Width <number>
Set LetterSpacing <float>
Set LineHeight <float>
Set TypingSpeed <time>
Set Theme <json|string>
Set Padding <number>
Set Framerate <number>
Set PlaybackSpeed <float>
Set WaitTimeout <time>
Set WaitPattern <regexp>
```

## 實測發現（manual 未列但合法）

| 指令 | 實測 | 說明 |
|------|------|------|
| `MouseClick <button>` | ✅ `vhs validate` 通過 | manual 指令集未列出（manual 僅鍵盤指令），但實際解析合法。可能為新版本功能或未文件化。 |

> 注意：manual 的指令集清單**不含滑鼠指令**（無 MouseMove/MouseClick），但實測 `MouseClick left` 可通過 validate。tapedeck 的 `Click`/`MouseMove` 擴充指令（REQ-1.3）轉譯依賴此行為 — 需以 vhs 實際行為為準，不能只信 manual。

## 與 tapedeck 擴充指令的對應

| tapedeck .roll（REQ-1.3） | vhs .tape 轉譯 | 關係 |
|---------------------------|---------------|------|
| `Set Engine Auto\|VHS\|Native` | —（tapedeck 引擎選擇，不進 .tape） | tapedeck 專屬 |
| `Set Output "<path>"` | `Output <path>`（無引號） | 轉譯層 |
| `Set FPS <n>` | `Set Framerate <n>` | 轉譯層（vhs 無 FPS 設定名） |
| `Sleep <Ns\|Nms>` | `Sleep <Nms>`（統一 ms） | 轉譯層 |
| `Type "<text>"` | `Type "<text>"` | 透傳 |
| `Enter` / `Key Enter` | `Enter` | 透傳 |
| `Key Down` / `Key Up` | —（vhs 無按鍵按下/放開） | 無對應，未來 Native PTY 專屬 |
| `Key q`（單字母） | `Type "q"` 或 `Ctrl+...` | tapedeck 專屬（vhs 需其他表達） |
| `WaitWindow "<title>" timeout=` | —（vhs 無法依視窗標題等待） | tapedeck 自動化層專屬 |
| `TargetWindow "<title>"` | —（不進 .tape） | tapedeck 自動化層專屬 |
| `Roll <duration>` | —（tapedeck 滾動檢查迴圈） | tapedeck 自動化層專屬 |
| `Shortcut "Ctrl+S"` | `Ctrl+S` | 轉譯層（需解析字串） |
| `ExecBefore` / `ExecAfter` | —（不進 .tape） | tapedeck 自動化層專屬 |
| `Optimize <codec> key=val` | —（錄後優化，不進 .tape） | tapedeck 自動化層專屬 |
| `Click Left` | `MouseClick left` | 轉譯層（實測合法） |

## CLI 子指令

```
vhs [options] <tape>           # 執行 tape 檔
vhs help                       # 說明
vhs new <name>                 # 產生範例 tape
vhs publish <gif>              # 發佈到 vhs.charm.sh
vhs record [-s/--shell]        # 錄製鍵盤動作產生 tape
vhs serve                      # 啟動 VHS SSH 伺服器（遠端錄製）
vhs themes [--markdown]        # 列出可用主題
vhs validate <file>...         # 只驗證不執行（CI 用）
```

## 對 tapedeck 的影響（REQ-7 對應）

1. parser 轉譯層須能解析上方「vhs 指令全集」29 項 + 實測的 `MouseClick`（共 30 指令），並原樣透傳至 .tape（REQ-7.2）
2. `Set FPS` 無 vhs 對應 — 轉譯層必須改寫為 `Set Framerate`
3. `Set Output` 轉譯為 `Output`（無引號形式）— 注意引號處理
4. vhs 的 `Wait[+Screen] /<regexp>/` 與 tapedeck `WaitWindow "<title>"` 語意不同：前者等字串出現，後者等視窗出現（走 compositor IPC）— 兩者不相容，語法上以完整指令名區隔（Wait vs WaitWindow）
5. `Key q` 等 tapedeck 專屬鍵盤指令無 vhs 對應，僅在 Native PTY 引擎（未來）有效；在 VHS 引擎下應產生明確錯誤或轉譯為近似語法（待決策）
