# tasks.md — Re-roll 實作任務

依賴順序排列。每個任務完成後執行 `cargo build` + 相關測試；全程 `cargo fmt` / `cargo clippy` 0 警告。

## 前置（獨立票，可先於本 change-set 執行）

- **T0**：修復 HEAD 編譯 — `NiriCompositor::clip_to_output` E0599（0853c08 WIP 中間態遺留：呼叫端存在、實作未落地；盤點 WIP 意圖後補實作或改走 `window_on_output`）；順手清 `dispatcher.rs` 重複的 WindowSize 警告區塊（約 296–308 行，同段列印兩次）

## T1：dispatcher 抽 `execute_script`

- [ ] `RunOptions` + `pub async fn execute_script`（design §2），`run()` 改薄殼
- [ ] 驗證：既有引擎解析 / 優先序測試全綠（行為不變）

## T2：reroll 搜尋 + 計畫

- [ ] `src/engine/reroll.rs`：`find_rolls`（std 遞迴、排序）+ `build_plan`（stale 判定，design §4）
- [ ] `src/cli.rs`：`reroll` 子指令骨架（REQ-1）+ `--dry-run` 輸出計畫
- [ ] db.rs：`latest_by_source(path)` 唯讀查詢
- [ ] 單元測試：搜尋排序 / stale 判定（mock db）

## T3：批次執行 + 覆寫保護

- [ ] 順序迴圈（executor 注入式，design §7）+ temp 原子置換（REQ-2.4）
- [ ] `register` 登錄（REQ-2.2）+ `--clean`（REQ-3.1）+ 彙總與 exit code（REQ-1.3/REQ-3.2）
單元測試：失敗不中斷、exit code、temp 置換、stale skip 語意 — 全走 mock executor

## T4：端到端驗證（桌面，手動）

- [ ] examples/ 至少 2 支（vhs + native 各一）→ `reroll --dry-run` 計畫正確 → 實錄成功
- [ ] 圖譜：成功支自動登錄（source_roll 帶入）；`--stale-only` 第二次跑全部 skip
- [ ] Headless：vhs 支成功、Native 支彙總失敗（SCN-3）

## 完成定義（DoD）

- [ ] `cargo test` 全綠 + `cargo fmt` / `cargo clippy` 0 警告
- [ ] `--dry-run` 計畫正確；批次失敗不中斷、彙總 exit code 正確
- [ ] 圖譜同步：成功支自動登錄、`--stale-only` 只挑逾期、`--clean` 顯式才清
- [ ] 單支行為與 `tapedeck run` 一致（同一 `execute_script` 路徑，無旁路）
- [ ] 無新增 crate 依賴
