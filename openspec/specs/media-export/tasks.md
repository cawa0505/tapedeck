# tasks.md — 靜態媒體壓製實作任務

## T1：FfmpegAdapter 骨架

- [ ] `src/media/ffmpeg.rs`：trait 定義 + `FfmpegAdapter::new()` 實作
- [ ] `probe()`：偵測 palettegen/libwebp/ffmpeg 版本
- [ ] 單元測試：mock ffmpeg stdout/stderr（Resilience 原則 4，Mock Subprocess）
- [ ] 驗證：`cargo build` + `cargo test`

## T2：optimize 子指令

- [ ] `src/media/optimize.rs`：palettegen 雙 Pass + libwebp
- [ ] `src/cli.rs`：`optimize` 子指令（--format/--quality/--fps/--output/--dry-run）
- [ ] `src/lib.rs`/`src/main.rs` 註冊 media 模組
- [ ] XDG 輸出解析（REQ-5，純 std）
- [ ] 驗證：`tapedeck optimize examples/test_tui.gif --format webp --dry-run` 顯示正確指令鏈

## T3：時間點日誌（Native 後端）

- [ ] `src/media/timeline.rs`：`TimelinePoint` + JSONL 讀寫
- [ ] dispatcher Native 模式：執行 `Click`/`Type` 時 append `{"ms","command"}`（起錄 0 基準）
- [ ] 驗證：GUI 錄製後 `~/.local/state/tapedeck/*.timeline.jsonl` 存在且格式正確

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
