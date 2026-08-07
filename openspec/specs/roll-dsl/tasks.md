# tasks.md — .roll DSL 實作任務

依賴順序排列。每個任務完成後執行 `cargo build` + 相關測試驗證。

## T1：parser AST 擴充（roll_parser.rs）

- [ ] 新增 `Engine { Auto, Vhs, Native }`，取代/升級 `Mode`
- [ ] `ScriptCommand` 擴充：
  - `Key(String, u32)` 合併 KeyDown/KeyUp（相容 `Key Down 3`）
  - `ExecBefore(String)`、`ExecAfter(String)`
  - `WaitWindow(String, u64)`（timeout 解析 `Ns`/`Nms`，預設 10000）
  - `TargetWindow(String)`、`WindowSize(u32,u32)`、`Padding(u32)`、`Roll(u64)`
  - `Shortcut(String)`、`Optimize(String, Vec<(String,String)>)`
- [ ] `Script` 新增 `engine: Option<Engine>`、`shell: Option<String>`（Terminal 別名）
- [ ] `Set Engine/Output/FPS/Shell` 分派
- [ ] 舊別名：`Title`/`Mode`/`Output`/`FPS`/`Terminal`/`Enter`
- [ ] 單元測試：三份 examples 全部解析成功 + 欄位斷言

## T2：dispatcher 雙後端（dispatcher.rs）

- [ ] `run(args)`：dry-run 輸出引擎/輸出/摘要 → `resolve_engine()`（Auto→偵測）→ 分派
- [ ] `run_vhs`：ExecBefore → VHS DSL 轉譯 → 暫存 .tape → vhs 執行 → ExecAfter
  - [ ] `Set Framerate`（非 FPS）、`MouseClick left/right/middle`、字母 Key → `Type "q"`
  - [ ] `Roll(s)` → `Sleep Ns`
- [ ] `run_native`：ExecBefore → detect_compositor → WaitWindow 輪詢 → TargetWindow+Padding → wf-recorder + Roll 計時 → Shortcut → ExecAfter → Optimize
- [ ] 單元測試：VHS 轉譯正確性（無需實際呼叫 vhs）

## T3：compositor 容錯補強（wayland/compositor.rs）

- [ ] `#[serde(ignore_unknown_fields)]` 加到 NiriWindow/NiriLayout/NiriGeometry/SwayNode/SwayRect
- [ ] WaitWindow 輪詢（dispatcher 側呼叫，200ms 間隔）

## T4：驗證（手動）

- [ ] `cargo build` 零 error
- [ ] `cargo test` 通過（T1/T2 新增測試）
- [ ] `./target/debug/tapedeck run --dry-run examples/*.roll` 三份全成功
- [ ] `./target/debug/tapedeck run examples/test_tui.roll` 實際錄製 gif 成功（回歸）

## 完成定義（Definition of Done）

- [ ] 三份 examples 全部可 dry-run
- [ ] test_tui.roll 錄製回歸通過
- [ ] tui_zago.roll（vhs 後端）實際錄製成功
- [ ] 文件與實作一致（docs/scripting.md 如有出入一併更新）
