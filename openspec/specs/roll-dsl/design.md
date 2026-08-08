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

OQ-03 已定案（2026-08-08）：**新增 `RecordingEngine` trait 抽象層**。dispatcher 透過 trait 分派，不直接呼叫後端函式（對外宣稱架構一致，且符合 Resilience 原則 1 的上層對應 — 外部工具用 Compositor trait 適配、引擎用 RecordingEngine trait 抽象，兩層分工）。

```rust
/// 錄影引擎抽象層（上層引擎，非外部工具適配）
#[async_trait]
pub trait RecordingEngine {
    async fn prepare(&self, script: &Script) -> Result<()>;
    async fn record(&self, script: &Script) -> Result<()>;
    async fn cleanup(&self, script: &Script) -> Result<()>;
}

// 實作：
// pub struct VhsEngine;    // 轉譯 .roll → .tape → 本機 vhs 或 SSH vhs serve
// pub struct NativeEngine; // detect_compositor + wf-recorder 錄製
```

```rust
pub async fn run(args: RunArgs) -> Result<()> {
    let script = parse_roll_script(&args.script_file)?;
    if args.dry_run {
        // REQ-5：印出 engine/output/fps/指令摘要，不執行
        return Ok(());
    }
    let engine: Box<dyn RecordingEngine> = match resolve_engine(&script) {
        Engine::Vhs => Box::new(VhsEngine),
        Engine::Native => Box::new(NativeEngine),
    };
    engine.prepare(&script).await?;
    engine.record(&script).await?;
    engine.cleanup(&script).await?;
    Ok(())
}
```

> 實作任務對應：tasks.md 新增 T 項目 — 定義 trait + VhsEngine/NativeEngine 兩個實作（現有 run_vhs/run_native 遷移為 trait 方法，行為不變）。

### 4.1 vhs 後端（run_vhs）

#### TUI 雙軌架構

`Mode TUI` 的底層引擎路由為雙軌：

```plaintext
┌──────────────────────────────┐
│ .roll Script (Mode TUI)      │
└──────────────┬───────────────┘
               │
 ┌─────────────┴─────────────┐
 │ TUI Engine Router         │
 └──────┬─────────────┬──────┘
        │             │
        ▼             ▼
┌─────────────────────────┐ ┌─────────────────────────┐
│ 1. Local vhs (PTY)      │ │ 2. vhs serve (SSH)      │
├─────────────────────────┤ ├─────────────────────────┤
│ • 適用：本機桌面環境     │ │ • 適用：無頭伺服器/CI    │
│ • 機制：本機 vhs 執行    │ │ • 機制：SSH 連入 vhs     │
│   .tape 錄製             │ │   serve daemon          │
│ • 錄影：ANSI PTY → gif/  │ │ • 錄影：遠端執行 .tape  │
│   webm（vhs 渲染）       │ │   → 回傳輸出檔          │
└─────────────────────────┘ └─────────────────────────┘
```

- `vhs serve` 為官方 SSH 模式（v0.11.0 確認存在）：VHS 自身成為 SSH daemon，透過 VT100/ANSI 接管 PTY，無瀏覽器、無 DOM 渲染
- 沙盒隔離：錄影在獨立 SSH session 執行，不污染本機 shell 環境變數/工作目錄
- 遠端 CI/CD：可直接把 .roll 丟給遠端 vhs serve 執行
- **ttyd 非選項**：ttyd 是「terminal→web」分享工具（Libwebsockets + xterm.js），**不需要** Headless Chrome；但錄製需另配瀏覽器側截圖，架構上比 vhs serve 笨重，故不採用

執行流程：

1. 執行所有 `ExecBefore`
2. 轉譯輸入指令為 VHS DSL：
   - `Type(t)` → `Type "t"`
   - `Enter` / `Key(name, n)` → VHS 對應（`Down`×n / `Enter` / `Tab`；字母 → `Type "q"`）
   - `Sleep(ms)` → `Sleep Nms`
   - `MouseMove(x,y,_)` → `MouseMove x y`
   - `Click(Left)` → `MouseClick left`
   - `Roll(s)` → `Sleep Ns`（錄製時長）
   - `Set Framerate n`（注意：VHS 是 Framerate 不是 FPS）
3. 寫暫存 .tape → `vhs <tmp.tape>`（本機）或 SSH 至 `vhs serve`（遠端）
4. 執行所有 `ExecAfter`（失敗僅警告）

### 4.2 Native 後端（run_native）

1. 執行所有 `ExecBefore`
2. `detect_compositor()` → Box<dyn Compositor>
3. `WaitWindow`：每 200ms 輪詢 `find_window_geometry` 直到成功或逾時
4. `TargetWindow` → geometry；`Padding(n)` → `to_wf_recorder_arg(padding)`
5. `WindowSize` → （記錄於 AST，執行時若 compositor 支援則 resize，否則警告略過）
6. `wf-recorder -g <geometry> -f <output>`；錄製期間依序執行操作指令（輸入後端由 `InputBackend::detect()` 選擇：/dev/uinput 可寫 → UinputNative（鍵盤+滑鼠）、否則 wtype（鍵盤）），`Sleep` 控制時序；`Roll(s)` 為總錄製時長上限，操作序列執行完未到則補眠到到期
7. `Shortcut` → 組合鍵送出（uinput 或 wtype）；`Click`/`MouseMove` 在無 uinput（/dev/uinput 不可寫）時警告略過（能力偵測，T10）
8. `ExecAfter`（失敗僅警告）
9. `Optimize(codec, kv)` → ffmpeg 轉換（如 AV1 vaapi → av1_vaapi 編碼器）

## 5. Compositor 適配器（現有，補強）

- `Compositor` trait 已存在：`find_window_geometry` / `move_to_workspace`（compositor.rs）
- 補強點：
  - `#[serde(ignore_unknown_fields)]` 於 NiriWindow/SwayNode 等 struct（上游 JSON 新增欄位不致解析失敗）
  - WaitWindow 輪詢透過 trait 重複呼叫，不需新方法

## 6. XDG 路徑解析（REQ-6）

統一路徑解析 helper（`src/xdg.rs` 或 `src/paths.rs`），三個 XDG 目錄共用 `$VAR` / `$HOME` fallback 邏輯：

```rust
/// $XDG_*_HOME/<sub> 或 $HOME/<fallback>/<sub>
fn xdg_dir(var: &str, fallback: &str, sub: &str) -> PathBuf {
    let base = std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(|h| PathBuf::from(h).join(fallback))
                .unwrap_or_else(|| PathBuf::from("/tmp"))
        });
    base.join(sub)
}
```

| 用途 | var | fallback | sub |
|------|-----|----------|-----|
| 輸出（快取） | `XDG_CACHE_HOME` | `.cache` | `tapedeck` |
| config | `XDG_CONFIG_HOME` | `.config` | `tapedeck/config.toml` |
| state（DB/歷程） | `XDG_STATE_HOME` | `.local/state` | `tapedeck` |

輸出路徑解析：

```rust
fn resolve_output_path(script_output: &str, cli_override: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = cli_override {
        return Ok(p.to_path_buf());              // CLI 顯式覆寫，照原樣
    }
    let p = Path::new(script_output);
    if p.is_absolute() {
        return Ok(p.to_path_buf());              // 絕對路徑照原樣
    }
    Ok(xdg_dir("XDG_CACHE_HOME", ".cache", "tapedeck").join(p))
}
```

- 解析後 `fs::create_dir_all(parent)` 再錄製（REQ-6.2）
- dry-run 印出解析後絕對路徑（REQ-6.3）
- config 讀取：`xdg_dir("XDG_CONFIG_HOME", ".config", "tapedeck/config.toml")`，不存在則用預設值（REQ-6.5）

## 7. 錯誤處理

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
