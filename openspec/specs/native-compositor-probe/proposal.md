# proposal.md — Native 引擎 compositor 偵測健壯化（native-compositor-probe）

## 動機（Motivation）

T4（GUI 原生錄製驗收）在 cybertron（umbriel session）直接失敗：
`detect_compositor()` 只靠 `XDG_CURRENT_DESKTOP` / `WAYLAND_DISPLAY` **字串比對**
判斷 compositor，umbriel 兩個變數都不含 "niri"/"sway"，靜默落入 fallback
`NiriCompositor`，之後 `niri msg --json windows` 每次呼叫都失敗，WaitWindow
輪詢把社群錯誤吞掉、10 秒後報「視窗未出現」——**歸因完全錯誤**（視窗明明開著，
foot 程序活著）。

依 openspec project.md 原則 2（版本探針與 Capability Check）：不要在錄到一半才
發現引數不對；啟動時先探測。偵測 compositor 的正確訊號是 **IPC socket 是否
可用**（`$XDG_RUNTIME_DIR` 下有無 niri/sway socket），不是桌麵環境變數字串，
更不是二進位是否存在。

死亡路徑（2026-09-26 實測，重現方式見 experiences.md）：

```
detect_compositor() → 字串不符 → NiriCompositor（fallback）
  → find_window_geometry() → `niri msg` 連不上（無 niri socket）→ Err（io error）
  → WaitWindow 輪詢迴圈 `Err(_) => sleep(200ms)` 重複 50 次
  → bail!「WaitWindow 逾時：視窗「foot-t4」未出現」（錯誤歸因誤導）
```

## 問題（Problem）

1. **誤判**：未知 compositor 靜默落 niri fallback；應該是「無法判定」錯誤
2. **誤導**：IPC 失敗（io error）與「視窗未出現」（業務逾時）混在同一個錯誤訊息
3. **吞錯**：WaitWindow 輪詢 `Err(_)` 把根因（io error / stderr）丟掉

## 成功標準（Success Criteria）

1. umbriel session 上跑 GUI roll → **明確錯誤**：
   「無法偵測 compositor（XDG_CURRENT_DESKTOP=umbriel，非 niri/sway）；niri/sway
   socket 皆不存在」——不是 WaitWindow 逾時
2. niri session 上跑 GUI roll → 行為與今天完全一致（偵測結果不受影響）
3. WaitWindow 對「IPC 死了」與「視窗真的沒出現」的錯誤訊息可區分
4. 既有測試全綠；`tapedeck doctor` 顯示 compositor 偵測結果（新增檢查項）
