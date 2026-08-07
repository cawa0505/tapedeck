# tasks.md — .roll DSL 實作任務

依賴順序排列。每個任務完成後執行 `cargo build` + 相關測試驗證。

## 執行優先序（CLI + 核心共用元件優先）

1. **T6 硬體探針**（核心共用：encoder fallback 鏈，供 `optimize` 選用 codec）
2. **T7 SQLite 資產圖譜**（CLI：`clean`/`link` 子指令 + DB 共用元件）
3. **DoD 驗證**（`tui_zago.roll` 實際錄製，vhs 後端收尾）
4. **T4b 殘項**（`--max-size` 定案，[待討論]）

## T1：parser AST 擴充（roll_parser.rs）

- [ ] 新增 `Engine { Auto, Vhs, Native }`，取代/升級 `Mode`
- [ ] `ScriptCommand` 擴充：
  - `Key(String, u32)` 合併 KeyDown/KeyUp（相容 `Key Down 3`）
  - `ExecBefore(String)`、`ExecAfter(String)`
  - `WaitWindow(String, u64)`（timeout 解析 `Ns`/`Nms`，預設 10000）
  - `TargetWindow(String)`、`WindowSize(u32,u32)`、`Padding(u32)`、`Roll(u64)`
  - `Shortcut(String)`、`Optimize(String, Vec<(String,String)>)`
- [ ] vhs 指令全集透寫（REQ-7.1）：`Require`/`Ctrl`/`Alt+key`/`Escape`/`Space`/`Backspace`/`Delete`/`Insert`/`Down`/`Left`/`Right`/`Tab`/`Up`/`PageUp`/`PageDown`/`ScrollUp`/`ScrollDown`/`Hide`/`Show`/`Wait /regexp/`/`Source`/`Screenshot`/`Copy`/`Paste` 原樣解析為透寫指令
- [ ] `Script` 新增 `engine: Option<Engine>`、`shell: Option<String>`（Terminal 別名）
- [ ] `Set Engine/Output/FPS/Shell` 分派
- [ ] 舊別名：`Title`/`Mode`/`Output`/`FPS`/`Terminal`/`Enter`
- [ ] 單元測試：三份 examples 全部解析成功 + 欄位斷言 + vhs 全集指令透寫測試

## T2：dispatcher 雙後端（dispatcher.rs）

- [ ] `RecordingEngine` trait（OQ-03 定案）：`prepare/record/cleanup` lifecycle
- [ ] `VhsEngine` / `NativeEngine` 實作（現有 run_vhs/run_native 遷移為 trait 方法，行為不變）
- [ ] `run(args)`：dry-run 輸出引擎/輸出/摘要 → `resolve_engine()`（Auto→偵測）→ trait 分派
- [ ] `resolve_output_path(script_output, cli_override)`（REQ-6）：相對→XDG cache；絕對/CLI 覆寫→照原樣
- [ ] 錄製前 `create_dir_all` 目標目錄
- [ ] dry-run 與實際執行顯示解析後絕對輸出路徑
- [ ] `VhsEngine::record`：ExecBefore → VHS DSL 轉譯 → 暫存 .tape → vhs 執行 → ExecAfter
  - [ ] `Set Framerate`（非 FPS）、`MouseClick left/right/middle`、字母 Key → `Type "q"`
  - [ ] `Roll(s)` → `Sleep Ns`
- [ ] `NativeEngine::record`：ExecBefore → detect_compositor → WaitWindow 輪詢 → TargetWindow+Padding → wf-recorder + Roll 計時 → Shortcut → ExecAfter → Optimize
- [ ] 單元測試：VHS 轉譯正確性（無需實際呼叫 vhs）
- [ ] 單元測試：`resolve_output_path`（相對/絕對/CLI 覆寫/XDG_CACHE_HOME 未設定）

## T3：compositor 容錯補強（wayland/compositor.rs）✅ 完成

- [x] serde 未知欄位容錯：**無需屬性** — 實測 serde 預設忽略未知欄位（`docs/ref/serde-ignore-unknown-fields.md`）；Niri/Sway JSON 解析天然容錯（Resilience 原則 #3）
- [x] WaitWindow 輪詢：T2 dispatcher 已實作（200ms 間隔，10s 預設逾時）

## T4b：CLI run flag 接線（dispatcher.rs）✅ 完成

規格來源：`project.md:181` — `tapedeck run SCRIPT_FILE [--output] [--fps] [--max-size] [--gif|--webp] [--dry-run]`

- [x] `--fps FPS`：覆寫腳本 fps（優先序 CLI > 腳本 > config），轉譯層輸出 `Set Framerate <n>`
- [x] `--gif|--webp`：覆寫輸出路徑副檔名（vhs 以 Output 副檔名決定格式，見 docs/ref/vhs-tape-format.md:9）
- [ ] `--max-size MB`：[待討論] vhs 無 MaxSize 指令（已查 docs/ref/vhs-tape-format.md 無對應）— 選項：a) 僅提示無法直接套用 b) 由 optimize 後處理壓縮（屬 P1 media-export 範圍）c) 其他。實作前需定案
- [x] 單元測試：flag 覆寫優先序（CLI > 腳本 > config defaults）— fps ×3 + 格式 ×3

## T4：驗證（手動）✅ 完成

- [x] `cargo build` 零 error（零 warning 硬要求）
- [x] `cargo test` 通過（30 tests）
- [x] `./target/debug/tapedeck run --dry-run examples/*.roll` 三份全成功（engine/XDG 路徑正確）
- [x] `./target/debug/tapedeck run examples/test_tui.roll` 實際錄製 gif 成功（回歸通過）

## T5：XDG config 讀寫（config.rs + paths.rs）✅ 完成

- [x] 新增 `src/paths.rs`：共用 XDG helper（`xdg_dir` / `cache_dir` / `config_path` / `resolve_output_path`），dispatcher 改用共用版本（REQ-6）
- [x] 新增 `src/config.rs`：`config_path()` 用 `xdg_dir("XDG_CONFIG_HOME", ".config", "tapedeck/config.toml")`（REQ-6.5）
- [x] `Config` struct：`[defaults]`（output、engine、fps、encoder）+ `[system.detected]`（encoder 清單、vaapi、dri）
- [x] 讀取：檔案不存在 → 回傳預設值並提示路徑；存在 → 解析（未知欄位忽略）
- [x] 單元測試：config 路徑（XDG set/unset）、輸出路徑解析（相對/絕對/CLI 覆寫/XDG set/unset）
- [x] `run()` 載入 config 並套用 `[defaults]`（腳本未指定時）

## T6：硬體探針（engine/probe.rs）✅ 完成（5427655）

- [x] `engine/probe.rs` 實作 `HardwareCapabilities::probe_system()`：
  - [x] ffmpeg 編碼器掃描（`ffmpeg -encoders`）：av1_vaapi → vp9_vaapi → libvpx-vp9 存在性
  - [x] `/dev/dri` 檢查（VA-API 裝置存在與否）
- [x] `encoder_fallback(probe, requested)`：依探針結果三階降級（AV1 HW → VP9 HW → VP9 SW），未探測到則直接 SW
- [x] config 寫入 `[system.detected]`（probe 產出後寫回，含 `save()`）
- [x] 單元測試：fallback 鏈（mock probe）+ save/upsert（段落級更新，保留註解）
- [x] 掛載 `pub mod probe;`（空殼已於 T5 移除，無殘留）
- [x] 測試序列化：crate::TEST_ENV_LOCK（跨模組 XDG env race 修正）

## T7：tapedeck doctor（src/doctor.rs）— 完成 ✅

規格來源：Resilience 原則 2（project.md）+ 用戶參考設計（結構化 deps 表）。依賴 T6 的 probe（doctor 是探針的 CLI 消費端，實作後解除 probe.rs/config.rs 的 `allow(dead_code)`）。

- [x] 新增 `src/doctor.rs`：`run_doctor()` — 結構化 deps 表（名稱、檢查指令、用途說明 Hint）
  - [x] deps 表：vhs（`--version`）、ffmpeg（`-version`）、wf-recorder（`-v`）— 用 `--version` 實作檢查（非 which，可偵測損壞/權限不足）
  - [x] 靜默執行（stdout/stderr 丟棄），只關心存不存在
  - [x] 逐項輸出 ✅ OK / ❌ MISSING + Hint（解釋為何需要該工具）
  - [x] 結束時總評（all systems go / missing 列表）
  - [x] 調用 `probe::probe_system()` + `config::save()` 寫回 `[system.detected]`
- [x] `cli.rs` 新增 `doctor` 子指令並接到 `run_doctor()`
- [x] 單元測試：mock 工具存在/缺失的輸出格式（不實際執行外部工具）
- [x] 完成後移除 probe.rs / config.rs 的 `#[allow(dead_code)]`（doctor 成為消費端）
- [x] 完成後更新 project.md 原則 2 狀態與變更集索引

## T8：SQLite 資產圖譜（src/db.rs）— 待實作

規格來源：OQ-04（project.md:132）+ Pillar 2（project.md:248）。MVP 範圍含 Asset Graph + 孤兒掃描；Re-roll 為 `[待討論]`（需新 change-set，非本次）。

- [ ] `Cargo.toml` 新增 `rusqlite`（bundled，不新增 directories 依賴，XDG 沿用 std 解析）
- [ ] 新增 `src/db.rs`：DB 初始化（`$XDG_STATE_HOME/tapedeck/tapedeck.db`，未設定 → `~/.local/state/tapedeck/`）
- [ ] `assets` 表：路徑、hash（sha256）、來源 .roll、mtime、影格快取目錄索引
- [ ] Markdown 引用追蹤：掃 `.md` 內 `assets/xxx.webm` 引用，建立 `.roll ➔ asset ➔ .md:行號` 三層關聯（Pillar 2）
- [ ] `clean` 指令（cli.rs 目前 bail 未實作）：孤兒掃描 — 列出並清除無 .md 引用的資產（dry-run 模式先列不刪）
- [ ] `link` 指令接上 DB 索引（media_link 目前獨立，需寫入 assets 表）
- [ ] 單元測試：DB 建表 + 資產登錄/查詢 + 孤兒掃描（temp dir + 隔離 XDG_STATE_HOME）
- [ ] 完成後更新 project.md Pillar 2 狀態與變更集索引

## 完成定義（Definition of Done）

- [ ] 三份 examples 全部可 dry-run
- [ ] test_tui.roll 錄製回歸通過
- [ ] tui_zago.roll（vhs 後端）實際錄製成功
- [ ] 文件與實作一致（docs/scripting.md 如有出入一併更新）
