# requirements.md — Re-roll 動態批次重錄

## REQ-1：CLI 子指令

- **REQ-1.1**：`tapedeck reroll [PATH]` — PATH 預設 cwd，遞迴搜尋 `*.roll`（依路徑排序，順序穩定）
- **REQ-1.2**：`--dry-run` 列出計畫（每支：路徑、解析引擎、輸出路徑），不錄製不登錄
- **REQ-1.3**：批次策略 — **順序執行**（輸入注入與錄製器皆不可並行）；單支失敗不中斷，結尾彙總 success/fail/skipped，有失敗時 exit code 非零

## REQ-2：單支重錄語意

- **REQ-2.1**：每支 .roll 走既有引擎路由（resolve_engine → RecordingEngine trait），行為與單次 `tapedeck run` 完全一致（零旁路，統一入口）
- **REQ-2.2**：成功後自動 `AssetTracker::register(output, source_roll)`（db 預留欄位 source_roll 正式啟用）
- **REQ-2.3**：`--stale-only`：僅重錄「.roll mtime > 上次登錄資產 mtime」者；從未登錄者視為 stale 一律重錄
- **REQ-2.4**：覆寫保護 — 新錄製先寫暫存路徑，成功後原子置換目標（錄一半失敗不摧毀舊資產）

## REQ-3：圖譜同步

- **REQ-3.1**：孤兒清理僅在 `--clean` 顯式要求時執行（預設不刪除，防誤殺）
- **REQ-3.2**：彙總輸出 — 每支一行（✅/❌/⏭️）+ 總計

## REQ-4：執行環境限制（誠實宣告）

- **REQ-4.1**：Native 軌重錄即真實桌面錄製，批次期間使用者不可干擾目標視窗 — 批次開始前列印提示
- **REQ-4.2**：無 Wayland session 時 Native 支失敗計入彙總（vhs 支不受影響，不整批 rollback）

## SCN 情境

- **SCN-1**：改了 examples/ 三支腳本文案 → `tapedeck reroll --stale-only` → 只重錄三支，圖譜自動更新
- **SCN-2**：`tapedeck reroll --dry-run` → 計畫表：5 支（3 vhs + 2 native），引擎與輸出路徑一目了然
- **SCN-3**：Headless（無顯示器）→ vhs 支成功、Native 支失敗計入彙總，exit 非零
- **SCN-4**：某支腳本語法錯誤 → ❌ 記入彙總，批次繼續

## 非目標（Non-Goals）

- 並行重錄（錄製器與輸入注入互斥）
- 靜默 workspace 接線（`Compositor::move_to_workspace` 已存在但屬 Native 錄製品質議題，另案）
- 遠端 / SSH 批次重錄
