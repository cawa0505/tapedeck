# tapedeck 專案總覽（OpenSpec project.md）

> 由原 `Spec.md` 吸納整理（2026-08-08）。腳本語言規格已獨立為 `specs/roll-dsl/`，此處不再重複。

## 1. 總覽

tapedeck 是一個專為 Linux 環境設計的雙模媒體錄影工具，提供 CLI/TUI 介面與宣告式腳本語言（.roll），支援硬體加速編碼（AV1/VP9）與 XDG 規範儲存。

## 2. 系統架構

```plaintext
┌──────────────────────────────┐
│ User Interface              │
├──────────────────────────────┤
│ CLI (clap)          TUI (ratatui)│
├──────────────────────────────┤
│ Core Pipeline               │
│ ┌─────────────────────┐     │
│ │ Script Parser       │◄──┐ │
│ │ (.roll)             │   │ │
│ └─────────────────────┘   │ │
│ ┌─────────────────────┐   │ │
│ │ Engine Router       │   │ │
│ │ (VHS / Native)      │   │ │
│ └─────────────────────┘   │ │
│ ┌─────────────────────┐   │ │
│ │ Recorder Executor   │   │ │
│ │ (vhs/wf-recorder)   │   │ │
│ └─────────────────────┘   │ │
│ ┌─────────────────────┐   │ │
│ │ Asset Manager       │   │ │
│ │ (SQLite + XDG)      │   │ │
│ └─────────────────────┘   │ │
└──────────────────────────────┘
```

## 2.1 健壯防護（Resilience Architecture）

> 最核心的架構挑戰：**依賴敏感度（Dependency Fragility）**。tapedeck 底層依賴 niri msg、swaymsg、wf-recorder、ffmpeg、vhs — 這些外部工具更新時可能改 CLI 參數名稱、JSON 結構或輸出格式，造成 tapedeck 崩潰或**靜默失敗（Silent Failure）**。以下 4 大設計原則為強制規範。

### 原則 1：鬆散耦合與適配器模式（Adapter Pattern）

核心邏輯**不寫死**外部工具 CLI 參數，以 Rust Trait 抽象為適配器：

```plaintext
┌─────────────────────────┐
│ tapedeck Core Engine    │
└────────────┬────────────┘
             │ (Trait Interface)
 ┌───────────┼───────────┐
 ▼           ▼           ▼
┌──────────────┐┌──────────────┐┌──────────────┐
│ NiriV1Adapter││ SwayV1Adapter││WfRecorderAdapt│
└──────┬───────┘└──────┬───────┘└──────┬───────┘
       │ (niri msg)    │ (swaymsg)     │ (wf-recorder)
       ▼               ▼               ▼
 [ Niri Binary ] [ Sway Binary ] [ wf-recorder Binary ]
```

- 當上游 API 改變（如 `logical_geometry` 改名）→ 新增 `NiriV2Adapter`，核心 .roll 執行邏輯**完全不用動**（對應 memory #2889/#2896）
- 全部外部工具整合必須在 Trait 之後（memory #2893：直接 CLI 呼叫禁止）

### 原則 2：版本探針與 Capability Check（tapedeck doctor）

不要在錄到一半才發現引數不對。啟動時或執行 `tapedeck doctor` 時先探測：

- 檢查 vhs / ffmpeg / wf-recorder 是否存在（`--version` 實作檢查，非 which — 可偵測損壞或權限不足）
- 檢查 `ffmpeg -encoders` 是否包含 `av1_vaapi` 等硬體編碼器
- 檢查 `/dev/dri` 是否存在（VA-API 硬體加速的物理前提）
- 探測結果寫回 `~/.config/tapedeck/config.toml` 的 `[system.detected]` 段落（供 optimize 等後續指令讀取，避免每次執行重掃）

```rust
// src/doctor.rs
// 結構化 deps 表：新增依賴檢查只需加一行（名稱、版本旗標、用途說明）
static DEPS: &[Dep] = &[
    Dep { name: "vhs", version_flag: "--version", hint: "TUI 錄製與編排所需。" },
    Dep { name: "ffmpeg", version_flag: "-version", hint: "影片編碼與處理所需。" },
    Dep { name: "wf-recorder", version_flag: "-v", hint: "Wayland 螢幕錄製所需。" },
];
pub fn run_doctor() -> anyhow::Result<()> { /* 靜默執行 + ✅/❌ 逐項輸出 + 硬體探針寫回 */ }
```

實作狀態：T7 完成 ✅（`tapedeck doctor` 子指令已上線，44 tests 通過，`[system.detected]` 寫回驗證成功）。

### 原則 3：強型別 JSON 解析與容錯（Lenient JSON Parsing）

與 `niri msg --json` / `swaymsg -t get_tree` 通訊時，**絕不**因 JSON 多出未知欄位而解析失敗：

```rust
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(ignore_unknown_fields)] // 關鍵！上游新增欄位不致崩潰
pub struct NiriWindow {
    pub id: u64,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub layout: NiriLayout,
}
```

### 原則 4：模擬測試（Dry-Run / Mock Subprocess）

單元測試**不實際啟動** wf-recorder/niri，而是以外部工具 stdout/stderr 建立 Mock 資料庫：

- 整合測試（CI）：建立各版本 `niri msg --json` 樣板（Mock Payload），驗證 Bounding Box 解析
- 上游改版時，單元測試第一時間抓出問題

## 3. 決策紀錄（Decision Log）

> 來源：2026-08-08 規格審查。對照實際代碼後發現「宣稱已實作」與現況不符、或與定案語法衝突，共 9 項議案。以下保留**完整討論軌跡**（議案 → 選項 → MVP 決策），供未來回頭檢視；決策已同步至對應 change-set。

### OQ-01：TUI 引擎軌道 — ✅ 已決策

- **議案**：文件宣稱 TUI 錄製有「雙態執行」— vhs Web 模式之外再加 Native PTY 第三軌（kitty 開真實終端 + wtype 注入 + wf-recorder 側錄），宣稱能錄「100% 真實 GPU 渲染」終端畫面。
- **選項**：a) 維持 vhs 雙軌（Local vhs + vhs serve SSH） b) 增列 Native PTY 第三軌 c) 延後至社群。
- **MVP 決策（a）**：維持 vhs 雙軌（design.md §4.1）。Native PTY 第三軌**不納入 v0.1**；若社群需求浮現，另開 change-set。
- **理由**：vhs 已覆蓋 95% TUI 錄製需求；Native PTY 需視窗鎖定 + PTY 配對 + wtype 重定向，複雜度高且非殺手級差異。

### OQ-02：GUI 鍵鼠輸入注入工具鏈（wtype / libei / uinput）— ✅ 已實作（T9 + T10）

- **議案**：文件宣稱「底層自動對接 wtype/libei（鍵盤）、ydotool/uinput（滑鼠）、hyprctl（視窗）」；實際無任何輸入注入代碼，GUI 自動化指令僅能轉譯進 .tape 給 vhs（TUI 場景）。
- **技術現實**：Wayland 安全模型禁止任意全域輸入注入；wtype 依賴 compositor 支援、ydotool 需 uinput 群組權限。調研（lib-1）結論：niri/sway 皆支援 `zwp_virtual_keyboard_manager_v1`（wtype）與 libei，ydotool 需 root、xdotool X11-only。
- **定案（T9 + T10）**：輸入後端採 `InputBackend::detect()` 分層 — **UinputNative 優先**（`src/engine/input.rs`，evdev crate 0.13.2，/dev/uinput 可寫時鍵盤+滑鼠全包，ref：docs/ref/uinput-rust-crates.md），**wtype 回退**（鍵盤，滑鼠略過）。doctor 含 Input Provider Diagnostic（裝置/核心模組/權限檢查 + 權限不足提示 usermod/udev rule）。
- **增強定案（uinput，2026-08-08）**：uinput（/dev/uinput）在 kernel 層註冊虛擬鍵盤/滑鼠，全 compositor 通用（含 GNOME/KDE 不吃 zwp_virtual_keyboard 的情況），Rust 有成熟封裝（evdev-rs / input-linux，調研 lib-4 進行中）。分層：`InputBackend::detect()` 優先 uinput（可寫 /dev/uinput）→ 回退 wtype（鍵盤，滑鼠略過）。doctor 增 Input Provider Diagnostic（檢查 /dev/uinput 存在、寫權限、kernel module、選定 backend），權限不足提示 `sudo usermod -aG input $USER`；udev rule 方案（99-input.rules, MODE=0660 GROUP=input）詢問用戶後寫入。
- **選項**：a) 僅 compositor 原生注入 b) 納入 wtype + libei c) GUI 自動化延後，v0.1 只做視窗鎖定錄製。
- **MVP 決策（b）**：採用 **wtype + libei**（鍵盤 wtype、滑鼠 libei），無需 root、niri/sway 支援成熟、libei 為 Emulated Input 新標準。ydotool/xdotool 排除。
- **參考**：`docs/ref/wayland-input-injection.md`（完整事實表與來源）。

### OQ-03：RecordingEngine trait 抽象層 — ✅ 已決策

- **議案**：文件宣稱已有 `RecordingEngine` trait（prepare/record/cleanup）與 `MediaOptimizer`/`AssetTracker`；實際 engine/mod.rs 無此 trait，dispatcher 直接分派。
- **選項**：a) 新增 `RecordingEngine` trait（構造 API，符合 Resilience 原則 1） b) 保持直接 dispatch（先跑起來，後期再抽象）。
- **MVP 決策（a）**：新增 `RecordingEngine` trait，dispatcher 改為 trait 分派，現有 run_vhs/run_native 遷移為 VhsEngine/NativeEngine 實作（行為不變）。外部工具用 Compositor trait 適配、上層引擎用 RecordingEngine trait 抽象，兩層分工。
- **落點**：`specs/roll-dsl/design.md` §4（trait 定義 + 分派示意）、tasks.md 新增實作任務。

### OQ-04：SQLite 資產圖譜（tapedeck.db）— ✅ 已決策

- **議案**：文件宣稱 tapedeck.db 資產↔.md 引用關聯圖譜、秒級孤兒清理、影格快取索引；實際無 rusqlite 依賴、無 DB 代碼。
- **選項**：a) 納入 v0.1 完整資產圖譜（核心賣點） b) 精簡（僅 assets 表 + clean） c) 延後。
- **MVP 決策（a）**：Cargo.toml 新增 `rusqlite`（bundled）；新增 `src/db.rs` — `assets` 表（路徑/hash/來源 .roll）+ Markdown 引用追蹤 + 影格快取索引；`clean` 指令執行孤兒掃描。XDG 路徑沿用既有 std 解析（不新增 directories 依賴）。
- **落點**：`src/db.rs`、`Cargo.toml`、`src/cli.rs`、`specs/roll-dsl/tasks.md`。

### OQ-05：硬體探針與編碼 Fallback 鏈 — ✅ 已決策

- **議案**：文件宣稱 probe_system() 掃描 VA-API 並寫入 config.toml `[system.detected]`、執行期三階降級（AV1 HW → VP9 HW → VP9 SW）；實際 probe.rs 為 2 行空殼、config 讀寫未實作。
- **選項**：a) 完整實作（probe + config + fallback，滿足 Resilience 原則 2） b) 精簡（僅 doctor 檢查） c) 延後。
- **MVP 決策（a）**：probe.rs 實作 ffmpeg 編碼器掃描（av1_vaapi → vp9_vaapi → libvpx-vp9）+ /dev/dri 檢查；config.toml `[system.detected]` 寫入 + `[defaults]` 讀取；執行時依 fallback 鏈降級。
- **落點**：`src/engine/probe.rs`（實作）、`src/config.rs`（新增，XDG config 讀寫）、tasks.md 新增任務。

### OQ-06：TUI 功能範圍 — ✅ 已決策

- **議案**：文件宣稱 fzf 雙欄選單 + Sixel/Kitty 即時預覽 + 逐幀 scrub + sprite 導出；實際 tui/mod.rs 153 行骨架，`PreviewMode::Image` 為佔位文字，無 nucleo/ratatui-image 整合。
- **選項**：a) v0.1 基礎（檔案列表 + 預覽佔位） b) v0.2 完整（fzf 雙欄 + 真 Sixel 預覽 + scrub） c) 依里程碑排程。
- **MVP 決策（b）**：v0.1 完整 TUI 導播台：啟用 preview feature（ratatui-image/image）、nucleo fzf 雙欄、Sixel/Kitty 即時預覽、逐幀 scrub（h/l）、sprite 導出（s 鍵）、y 鍵複製 Markdown、e 鍵 $EDITOR、r 鍵執行。
- **落點**：`src/tui/mod.rs`（大幅實作）、`Cargo.toml`（啟用 preview feature）、tasks.md 新增任務。

### OQ-07：MCP 工具實作 — ✅ 已決策

- **議案**：文件宣稱原生 JSON-RPC stdio 對接 Cursor/OpenCode，Agent 錄完自動驗證 UI；實際 src/mcp/ 的 ToolManager 是 stub（寫 placeholder 檔）。
- **選項**：a) v0.1 完整工具集（record_roll/link/optimize/clean 真正整合） b) 精簡（僅 record_roll） c) 延後（Agent 直接呼叫 tapedeck run）。
- **MVP 決策（a）**：JSON-RPC stdio 伺服器實作四工具並與 dispatcher 真正整合（非 stub）、main.rs 掛載 `mod mcp`。閉環定位：Agent 錄製 → PNG 影格 → Vision 驗證。
- **落點**：`src/mcp/`（tools.rs 完整實作）、`src/main.rs`（掛載）、tasks.md 新增任務。

### OQ-08：.roll 語法擴充（Engine 關鍵字 / delay / Scroll / Optimize 形式）— ✅ 已決策

- **議案**：審查文件出現多種未定案語法：`Engine Wayland`（新別名）、`Type "..." delay=40ms`（人性化節奏）、`Scroll Down 5`（滾輪）、`Optimize WebM encoder=...`（容器先決）等，與定案語法（`Set Engine`/`Type`/`Optimize <codec>`）衝突。
- **選項**：a) 維持定案語法（衝突語法全數不採） b) MVP 支援所有 vhs 原語法 + 加入我們想定義的規劃（delay/Scroll 等留空間）。
- **MVP 決策（b）**：支援 vhs 指令全集（REQ-7.1，轉譯層原樣透傳）+ tapedeck 擴充指令；`Engine Wayland`/`delay`/`Scroll`/`Optimize WebM` **不納入**，未來確定需求時另開 change-set。REQ-7 由 [待討論] 改為定案。
- **落點**：`specs/roll-dsl/requirements.md` REQ-7。

### OQ-09：v0.1 Scope 邊界（作業系統/編碼器/儲存）— ✅ 已決策

- **議案**：定案邊界 = Linux (Wayland)、VA-API AV1/VP9 + SW fallback、XDG + SQLite、CLI+TUI+MCP；但文件另宣稱含「X11 (ffmpeg) 引擎」、「NVENC/AMF 社群貢獻」、MP4 容器與 h264 fallback 鏈 — 對外宣稱不一致。
- **選項**：a) 明示 X11 為社群貢獻（v0.1 僅 Wayland） b) v0.1 即支援 X11 c) 文件移除 X11 提及。
- **MVP 決策（a）**：v0.1 僅 Linux (Wayland)；X11/ffmpeg、NVENC/AMF、macOS/Windows 全列為社群貢獻（README Roadmap + RecordingEngine trait 擴充點）。MP4 容器與 h264 fallback 鏈不納入 v0.1（webm 為唯一輸出容器）。
- **落點**：`README.md`（Roadmap 明示社群貢獻邊界）、§2 架構圖註記（X11 = 社群貢獻）。

## 4. 既有模組設計（實作參考）

### 3.1 CLI 引擎

**指令結構**（clap derive）：

```bash
tapedeck run SCRIPT_FILE [--output PATH] [--fps FPS] [--max-size MB] [--gif|--webp] [--dry-run]
tapedeck link MEDIA_FILE [--format zola|md|html]
tapedeck optimize INPUT -o OUTPUT [--quality 1-100]
tapedeck clean [--dry-run]
```

**錯誤處理**：
- 檔案不存在 → 回傳 `Err(FileNotFound)`
- 編碼失敗 → 自動降級至軟體編碼並警告

### 3.2 TUI 導播台

**布局**（ratatui + fzf）：

```plaintext
┌───────────────────────────────────────────────────────────────┐
│ TAPEDECK v0.1.0 [REC 00:00] [WAYLAND]                        │
│ > 01_demo.roll                                                │
│   01_demo.gif │ ┌────────────── Preview ────────────┐        │
│               │ │ [Sixel/Kitty Rendered Preview]   │        │
│               │ └───────────────────────────────────┘        │
│               │ Type: GIF | Size: 2.4MB | Linked: 2x        │
├───────────────────────────────────────────────────────────────┤
│ [/] Filter [e] Edit [r] Run [y] CopyMD [o] Open [c] Clean      │
└───────────────────────────────────────────────────────────────┘
```

**快捷鍵**：
- `e`：編輯腳本
- `r`：背景執行
- `y`：複製 Markdown 語法
- `o`：系統預設播放器開啟
- `c`：標記孤兒資產

## 5. 效能與限制

| 指標                | 目標值                  |
|---------------------|------------------------|
| 錄影延遲            | < 100ms (硬體編碼)     |
| 最大支援解析度      | 3840x2160 (4K)         |
| 孤兒資產掃描速度    | < 1s (SQLite 增量索引)  |
| TUI 渲染 FPS        | ≥ 60 FPS               |

## 6. 安全性與隱私

- **XDG 規範**：配置檔與快取不污染 `$HOME`（config: `~/.config/tapedeck/`、state: `~/.local/state/tapedeck/`、cache: `~/.cache/tapedeck/`）
- **權限隔離**：錄製流程以使用者權限執行
- **資料庫加密**：tapedeck.db 未來支援 SQLCipher

## 6.1 功能支柱（Product Pillars）

產品定位為「內容工程工作檯」，四大功能支柱。覆蓋度分析：P2/P3/P4 已由 OQ-04/07/06 定案（決策層級），P1 與各支柱的執行細節（工具參數、資料模型、UX 流程）尚未寫入變更集規格。

### Pillar 1：影格處理與膠捲生成（Filmstrip）🎞️ — `[待討論]`

**Smart GIF/WebP Export**：錄完後自動壓製靜態媒體（供 README/Medium 嵌入）：
- palettegen 雙重 Pass 調色盤生成（高畫質、零雜訊、小體積）
- 輸出：GIF 或 WebP

**Markdown 膠捲步驟圖（Filmstrip Step Sheet）**：
- 依 .roll 內 `Click`/`Type` 時間點自動切出 3~5 張代表性 PNG 影格
- 自動拼成橫向「操作步驟分解圖」（靜態圖文教學）

- **對應**：OQ-03（MediaOptimizer 角色）、OQ-04（影格快取索引）
- **現況**：✅ 文件已建立（`specs/media-export/` 四件套，T1~T5 任務分解）；3 個執行細節 `[待討論]`（操作點合併閾值、影格間距/標籤、Screenshot 編號對應）
- **落點**：`src/media/`（ffmpeg.rs + optimize.rs + filmstrip.rs + timeline.rs）、ffmpeg 適配器、Cargo.toml（無新增依賴）

### Pillar 2：SQLite 資產圖譜（tapedeck db）🗄️ — ✅ OQ-04 已定案（Re-roll `[待討論]`）

- **引用追蹤（Asset Graph）**：`Demo.roll ➔ assets/demo.webm ➔ docs/README.md:42` 三層關聯
- **孤兒掃描（tapedeck clean）**：掃 .md 引用，清除無引用廢棄影片
- **Re-roll（動態批次重錄）**：`tapedeck reroll` 搜尋專案所有 .roll，於背景 Niri/Sway Workspace 全部重錄 — `[待討論]`（OQ-04 未含此指令，需新 change-set）
- **落點**：`src/db.rs`（T8 完成 ✅ — `link` 登錄 + `clean` 孤兒掃描，.md 行號引用追蹤；Re-roll 子指令 `tapedeck reroll` 待議）

### Pillar 3：MCP 視覺自我驗證閉環 🤖 — ✅ OQ-07 已定案

- **record_and_inspect Tool**：Agent 呼叫執行 .roll
- **影格抽樣回傳（Vision Feedback）**：錄完抽 3 張關鍵影格（PNG/Base64）回傳 Agent Context，Agent 以 Vision LLM 自行驗證（按鈕顏色/Layout/文字輸入）
- **閉環**：錄製 → PNG 影格 → Vision 驗證，100% 自動化 E2E
- **落點**：`src/mcp/tools.rs`（定案範圍內）；影格抽樣演算法細節待 Pillar 1 定案後同步
- **進度**：✅ 已完成（T1-T4，commit efffaee）— 六工具全接線（`src/mcp/{server,tools}.rs` + `tests/mcp_stdio.rs` 協議測試 95 tests 綠）；T3 視覺閉環（3 幀 Base64 PNG + asset protocol）已實測；T5 文件同步完成

### Pillar 4：TUI 終端導播台（Sixel/Kitty 逐幀預覽）🎛️ — ✅ OQ-06 已定案

- **fzf 雙欄列表**：左 .roll 腳本 / 右已錄 .webm
- **逐幀滾動預覽（Frame Scrubbing）**：Sixel / Kitty Graphics Protocol 於終端內顯示影片，方向鍵左右逐幀查看
- **y 鍵一鍵複製**：`![](assets/demo.webm)` 或 Zola/Hugo Shortcode
- **落點**：`src/tui/mod.rs`（定案範圍內，見 §3.2 布局）

## 7. 未來擴展

1. **macOS/Windows 支援**（開放社群貢獻）
2. **雲端同步**（透過 MCP 協議）
3. **AI 導播**（自動偵測畫面變化優化影格率）

## 8. 驗證標準

- **單元測試**：覆蓋核心邏輯（引擎選擇、腳本解析）
- **E2E 測試**：使用 .roll 腳本驗證錄製流程
- **效能基準**：每月執行 `cargo criterion` 比較編碼速度

## 9. 錯誤碼表

| 代碼      | 意義                     | 處理策略                  |
|-----------|--------------------------|---------------------------|
| E001      | 腳本檔案不存在           | 終止並顯示檔案路徑        |
| E002      | 硬體編碼失敗             | 降級至軟體編碼並警告      |
| E003      | 視窗定位失敗             | 中止錄製並建議手動模式    |

## 10. Benchmark 工具

```bash
# 執行所有基準測試
cargo bench -- --output-format bencher
```

## 11. 變更集索引

| 變更集 | 狀態 | 摘要 |
|--------|------|------|
| `specs/roll-dsl/` | 已定案（T1–T8 完成 ✅） | .roll 語法定案、雙層執行（vhs 轉譯 + tapedeck 自動化）、XDG 路徑/設定、doctor 依賴檢查、SQLite 資產圖譜 |
| `specs/media-export/` | 文件已建立（待審閱） | P1 靜態媒體壓製：optimize 雙 Pass + filmstrip 步驟圖；3 項 `[待討論]`（見 §6.1） |
| §3 待確認決策 | **✅ 全部決策完成** | OQ-01（vhs 雙軌）、OQ-02（wtype+libei）、OQ-03（RecordingEngine trait）、OQ-04（完整 SQLite 資產圖譜）、OQ-05（probe+config+fallback 完整實作）、OQ-06（完整 TUI 導播台）、OQ-07（完整 MCP 工具）、OQ-08（vhs 全集 + 擴充指令）、OQ-09（v0.1 僅 Wayland，其餘社群貢獻） |
| §6.1 功能支柱 | **P2/P3/P4 定案、P1 文件已建立** | Pillar 1（media-export 四件套完成，3 項細節 `[待討論]`）；P2（資產圖譜）OQ-04、Re-roll 待議；P3（MCP 閉環）OQ-07；P4（TUI 導播台）OQ-06 |
