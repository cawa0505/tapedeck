# tasks.md — 靜態媒體壓製實作任務

依賴順序排列。每個任務完成後執行 `cargo build` + 相關測試驗證。

## 執行優先序（CLI + 核心共用元件優先）

1. **T1 FfmpegAdapter 骨架**（核心共用：optimize/filmstrip 的共同地基）
2. **T2 `optimize` 子指令**（CLI：palettegen 雙 Pass + libwebp，T1 之後）
3. **T3 時間點 JSONL**（核心共用：Native 後端時間戳，filmstrip 依賴）
4. **T4 `filmstrip` 子指令**（CLI：依賴 T1 + T3）
5. **T5 端到端驗證**

## T1：FfmpegAdapter 骨架

- [x] `src/media/ffmpeg.rs`：trait 定義 + `FfmpegAdapter::new()` 實作（實作 struct 為 `FfmpegV1Adapter`，依 #2896 命名慣例）
- [x] `probe()`：偵測 palettegen/libwebp/ffmpeg 版本
- [x] 單元測試：mock ffmpeg stdout/stderr（Resilience 原則 4，Mock Subprocess）
- [x] 驗證：`cargo build` + `cargo test`（52 tests，零警告）

## T2：optimize 子指令 ✅

- [x] `src/media/optimize.rs`：palettegen 雙 Pass + libwebp
- [x] `src/cli.rs`：`optimize` 子指令（--format/--quality/--fps/--output/--dry-run）
- [x] `src/lib.rs`/`src/main.rs` 註冊 media 模組
- [x] XDG 輸出解析（REQ-5，純 std）
- [x] 驗證：`tapedeck optimize` dry-run 顯示正確指令鏈 + 實際 webm→gif / gif→webp 轉換成功

## T3：時間點日誌（Native 後端）

- [x] `src/media/timeline.rs`：`TimelinePoint` + JSONL 讀寫
- [x] dispatcher Native 模式：寫入 `~/.local/state/tapedeck/<stem>.timeline.jsonl`
  - 實作偏差：OQ-02 輸入注入未接線（NativeEngine 不執行 Click/Type），時間點以**腳本時序推算**（走訪指令累加 Sleep，Click/Type 處記錄 ms）；注入實作後改為實際執行時記錄
- [x] 驗證：JSONL 格式（單元測試 roundtrip 覆蓋）；GUI 端到端留待 Native 錄製環境

## T4：filmstrip 子指令

- [ ] `src/media/filmstrip.rs`：抽樣（操作點 → 合併 <500ms → `ffmpeg -ss` 抽幀）
- [ ] vhs 轉譯層注入 `Screenshot "frames/NN.png"`（dispatcher `script_to_tape_content`）
- [ ] `hstack` 拼接 + pad 間距
- [ ] `src/cli.rs`：`filmstrip` 子指令（--roll/--count/--output/--dry-run）
- [ ] fallback：無 .roll / 無日誌 → 等間距抽樣
- [ ] 驗證：`tapedeck filmstrip <錄製> --roll examples/test_tui.roll --dry-run` 正確

## T5：端到端驗證

- [ ] 真實錄製 `examples/test_tui.roll` → `optimize` → GIF 成功（小體積、無雜訊）
- [ ] 真實錄製 → `filmstrip` → 橫向步驟圖 PNG 成功
- [ ] `cargo fmt` / `cargo clippy`（0 警告）/ `cargo test` 全過
- [ ] 文件與實作一致（AGENTS.md 文件優先）

## 完成定義（DoD）

- [ ] optimize 雙 Pass 輸出的 GIF/WebP 體積明顯小於原 WebM
- [ ] filmstrip 依 `Click`/`Type` 時間點輸出 3~5 張步驟圖
- [ ] 三種時間點來源（vhs 注入 / JSONL / fallback）皆可運作
- [ ] 全程 XDG 合規，`--output` 可覆寫
- [ ] 無新增 crate 依賴
