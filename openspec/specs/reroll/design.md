# design.md — Re-roll 動態批次重錄

## 1. 元件分工

既有一等公民全部複用，reroll 自身只是「批次迴圈 + 圖譜同步」：

```plaintext
reroll（src/engine/reroll.rs，新；< 400 行）
 ├─ 搜尋：find_rolls(root) → Vec<PathBuf>（std::fs 讀目錄遞迴，不新增依賴）
 ├─ 計畫：build_plan(rolls, stale_only, db) → Vec<RerollItem>
 ├─ 執行：dispatcher 共用入口（§2）
 └─ 同步：AssetTracker::register / clean（既有 db API）
```

## 2. 執行入口重構（唯一的新公共路徑）

現狀 `dispatcher::run(args: RunArgs)` 是 CLI 專屬入口。reroll 需要「給一個 .roll 路徑，回傳輸出路徑」的語意：

- 抽出 `pub async fn execute_script(script_file: &Path, opts: &RunOptions) -> Result<PathBuf>`；`RunOptions` 僅含 output 覆寫（reroll 一律用腳本預設）
- `run()` 改為薄殼呼叫 `execute_script()`，reroll 亦然 → 單支行為與 `tapedeck run` **保證一致**
- reroll 禁止自行重寫引擎邏輯（OQ-03 trait 分派是唯一路徑，不得旁路）

## 3. 覆寫保護與輸出判定

- 目標輸出路徑 = `execute_script` 既有邏輯決定（腳本內 Output / 預設 XDG cache）
- 暫存：輸出寫 `<target>.reroll-tmp`，record 成功後 `fs::rename` 置換
- rename 失敗（跨裝置罕見）→ fallback copy + delete；暫存與目標同 XDG 子樹時理論上不觸發

## 4. stale 判定（db 預留欄位啟用）

`assets` 表既有 `source_roll`（來源腳本）與 `mtime`（資產檔 mtime）欄位，註明「Re-roll 消費端未實作」。啟用方式：

- 計畫階段查詢該 .roll 對應最新一筆登錄資產：`.roll` mtime > 資產 mtime → stale
- 無登錄記錄 → stale
- db.rs 新增查詢 `latest_by_source(path)`（唯讀，不動 schema）

## 5. CLI（clap derive）

```text
tapedeck reroll [PATH] [--stale-only] [--clean] [--dry-run]
```

## 6. 錯誤處理

| 狀況 | 行為 |
|------|------|
| 單支語法 / 引擎 / 錄製失敗 | ❌ 計入彙總，繼續下一支 |
| 暫存置換失敗 | ❌ 計入彙總，保留 temp 供除錯 |
| db open 失敗 | 整批 bail（圖譜同步是核心承諾） |
| 全部 skip（--stale-only 無 stale） | exit 0，印「nothing to do」 |

## 7. 限制（不演）

- Native 支重錄 = 真實桌面錄製，批次期間使用者不可動目標視窗；未來可接 `Compositor::move_to_workspace`（trait 已有、Niri 實作 `#[allow(dead_code)]` 待接線）改善干擾，屬另案
- 批次迴圈以 executor 抽象（注入式）隔離測試，不依賴真實錄製
