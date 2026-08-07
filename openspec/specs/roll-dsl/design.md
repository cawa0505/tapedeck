# design.md — .roll DSL 技術設計

## 1. 架構總覽

```plaintext
┌─────────────────────────────────────────────┐
│ tapedeck Core                               │
│  ┌───────────────┐   ┌───────────────────┐  │
│  │ roll_parser   │──▶│ Script (AST)      │  │
│  └───────────────┘   └─────────┬─────────┘  │
│                                ▼            │
│  ┌───────────────────────────────────────┐  │
│  │ dispatcher (引擎選擇 + 自動化層執行)   │  │
│  └──────────┬──────────────────┬─────────┘  │
│             ▼                  ▼            │
│  ┌─────────────────┐   ┌─────────────────┐  │
│  │ vhs 轉譯層       │   │ Native 後端      │  │
│  │ Script→.tape DSL │   │ compositor 偵測  │  │
│  │ └ vhs 執行       │   │ wf-recorder 執行 │  │
│  └─────────────────┘   └─────────────────┘  │
└─────────────────────────────────────────────┘
```

## 2. AST 資料模型（roll_parser.rs）

```rust
pub enum Engine { Auto, Vhs, Native }

pub enum ScriptCommand {
    // --- 輸入層（vhs 轉譯）---
    Type(String),
    Enter,
    Key(String, u32),        // 按鍵名稱（Down/Up/Enter/Tab/q...）, 次數
    Sleep(u64),              // ms
    MouseMove(i32, i32, Option<String>), // x, y, speed
    Click(ClickType),

    // --- 自動化層（tapedeck 執行）---
    ExecBefore(String),
    ExecAfter(String),
    WaitWindow(String, u64), // title, timeout_ms
    TargetWindow(String),
    WindowSize(u32, u32),
    Padding(u32),
    Roll(u64),               // 秒
    Shortcut(String),        // "Ctrl+S"
    Optimize(String, Vec<(String, String)>), // codec, [(k,v)...]
}

pub struct Script {
    pub title: Option<String>,
    pub engine: Option<Engine>,
    pub output: Option<String>,
    pub fps: Option<u32>,
    pub shell: Option<String>,   // Terminal/Set Shell
    pub commands: Vec<ScriptCommand>,
}
```

變更重點：
- `Mode` 從列舉升級為 `Engine` 別名（REQ-2.2：Mode TUI→Auto、Mode GUI→Native）
- `KeyDown(u32)`/`KeyUp(u32)` 合併為 `Key(String, u32)`，容納任意具名按鍵
- 移除 `Terminal` 指令 → `shell` 欄位（與 Output/FPS 同位階）
- 新增自動化指令列舉（ExecBefore/WaitWindow/.../Optimize）

## 3. Parser 分派（roll_parser.rs）

單一 `match`，關鍵字區分大小寫：

| 關鍵字 | 動作 |
|--------|------|
| `Set Engine/Output/FPS/Shell` | 設定欄位 |
| `Title` | 設定 title（舊別名） |
| `Mode` | 舊別名 → engine |
| `Output` / `FPS` / `Terminal` | 舊別名 → 對應欄位 |
| `Type` / `Enter` / `Sleep` | 輸入指令 |
| `Key <name> [count]` | Key(String, count|1) |
| `MouseMove <x> <y> [speed=..]` | MouseMove |
| `Click <Left\|Right\|Middle>` | Click |
| `ExecBefore` / `ExecAfter` | 自動化 |
| `WaitWindow "<t>" timeout=..` | WaitWindow（timeout 解析 `Ns`/`Nms`，預設 10s） |
| `TargetWindow` / `WindowSize` / `Padding` / `Roll` | 自動化 |
| `Shortcut` / `Optimize` | 自動化 |

## 4. Dispatcher（dispatcher.rs）

```rust
pub async fn run(args: RunArgs) -> Result<()> {
    let script = parse_roll_script(&args.script_file)?;
    if args.dry_run {
        // REQ-5：印出 engine/output/fps/指令摘要，不執行
        return Ok(());
    }
    let engine = resolve_engine(&script);  // Auto→偵測環境
    match engine {
        Engine::Vhs => run_vhs(&script).await,      // vhs 轉譯層
        Engine::Native => run_native(&script).await, // compositor + wf-recorder
    }
}
```

### 4.1 vhs 後端（run_vhs）

1. 執行所有 `ExecBefore`
2. 轉譯輸入指令為 VHS DSL：
   - `Type(t)` → `Type "t"`
   - `Enter` / `Key(name, n)` → VHS 對應（`Down`×n / `Enter` / `Tab`；字母 → `Type "q"`）
   - `Sleep(ms)` → `Sleep Nms`
   - `MouseMove(x,y,_)` → `MouseMove x y`
   - `Click(Left)` → `MouseClick left`
   - `Roll(s)` → `Sleep Ns`（錄製時長）
   - `Set Framerate n`（注意：VHS 是 Framerate 不是 FPS）
3. 寫暫存 .tape → `vhs <tmp.tape>`
4. 執行所有 `ExecAfter`（失敗僅警告）

### 4.2 Native 後端（run_native）

1. 執行所有 `ExecBefore`
2. `detect_compositor()` → Box<dyn Compositor>
3. `WaitWindow`：每 200ms 輪詢 `find_window_geometry` 直到成功或逾時
4. `TargetWindow` → geometry；`Padding(n)` → `to_wf_recorder_arg(padding)`
5. `WindowSize` → （記錄於 AST，執行時若 compositor 支援則 resize，否則警告略過）
6. `wf-recorder -g <geometry> -f <output>`，`Roll(s)` 到期後終止
7. `Shortcut` → 組合鍵送出（ydotool 或 compositor action，視可用性；不支援則警告）
8. `ExecAfter`（失敗僅警告）
9. `Optimize(codec, kv)` → ffmpeg 轉換（如 AV1 vaapi → av1_vaapi 編碼器）

## 5. Compositor 適配器（現有，補強）

- `Compositor` trait 已存在：`find_window_geometry` / `move_to_workspace`（compositor.rs）
- 補強點：
  - `#[serde(ignore_unknown_fields)]` 於 NiriWindow/SwayNode 等 struct（上游 JSON 新增欄位不致解析失敗）
  - WaitWindow 輪詢透過 trait 重複呼叫，不需新方法

## 6. 錯誤處理

- ExecBefore 失敗 → `bail!`（中止錄製）
- ExecAfter 失敗 → `warn` 繼續
- WaitWindow 逾時 → `bail!` 並提示手動模式（對應 project.md 錯誤碼表 E003）
- 外部工具缺失 → 明確錯誤訊息（提示 VHS_BIN/WF_RECORDER 環境變數）

## 7. 測試策略

- 單元測試：parser 對三份 examples 的解析斷言（含舊別名、timeout 單位、Key 計數）
- 單元測試：VHS DSL 轉譯的正確性（Framerate 命名、MouseClick 對應）
- 整合測試（CI）：`--dry-run` 對三份 examples 全數成功；mock 子程序（fake niri/swaymsg 輸出樣板）驗證 geometry 解析 — 對應 Resilience 設計原則 4

## 8. 檔案影響範圍

| 檔案 | 變更 |
|------|------|
| `src/engine/roll_parser.rs` | AST 擴充 + 新語法分派 |
| `src/engine/dispatcher.rs` | run() 分派 + 雙後端 |
| `src/engine/wayland/compositor.rs` | serde 容錯補強 |
| `src/tui/mod.rs` | 無（僅用 title/parser） |
| `examples/*.roll` | 不變（已是定案語法） |
