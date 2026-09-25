# proposal.md — Re-roll 動態批次重錄（tapedeck reroll）

## 動機（Motivation）

Pillar 2（SQLite 資產圖譜）的收尾指令。專案內大量 .roll 修改後（調文案、改解析度、換編碼器），目前只能一支支手動 `tapedeck run` 重錄，再手動 `link` 回圖譜。批次重錄把「重錄 → 登錄 → 清孤兒」壓成一條指令。

## 問題（Problem）

- .roll 變更後，圖譜內的舊資產與腳本脫鉤，需逐支手動重錄
- 重錄後忘記 `tapedeck link` → 圖譜退回孤兒狀態
- 無法盤點「哪些 .roll 逾期未重錄」— db 的 mtime 欄位已預留，無消費端

## 成功標準（Success Criteria）

1. `tapedeck reroll [PATH]` 遞迴搜尋 .roll，逐支依既有引擎路由重錄（與單次 `tapedeck run` 完全同路徑）
2. 每支成功重錄後自動登錄圖譜（`AssetTracker::register`）；單支失敗不中斷批次，結尾彙總
3. `--stale-only` 只重錄自上次登錄後有變更的 .roll（啟用 db 預留欄位）
4. `--dry-run` 列出重錄計畫（腳本、引擎、輸出路徑），不錄製不登錄
5. 批次全程不需人為介入（vhs 軌無頭可行；Native 軌限制見 requirements REQ-4）
