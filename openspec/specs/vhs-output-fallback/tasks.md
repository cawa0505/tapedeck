# tasks.md — vhs-output-fallback

> 執行者：code agent（zero）。每個 Task 一個 commit，順序執行。
> 驗證基準：`CARGO_HOME=$HOME/.cargo cargo test --offline` 全綠＋`cargo fmt`/`clippy` 過。

## T1 — sink codegen＋存在性偵測（`feat(engine): vhs frame sink 與缺失偵測`）

1. `VhsEngine` 增加 sink 路徑欄位（`temp_dir()/tapedeck-<pid>-<millis>-sink`）
2. `script_to_tape_content` 末尾附加 `Output "<sink>/frames.png"`（引號）；dry-run 不受影響
3. `record()` exit 0 後：target 存在且 >0 bytes → Ok；缺失 → 送 T2 fallback；
   兩路徑收尾皆刪 sink（best-effort）
4. 單元測試：tape 內容含 sink 行、sink 路徑唯一性、codegm 不改動 Script.commands

## T2 — 自力合成（`feat(engine): vhs 缺檔 fallback 自力合成`）

1. 新檔 `src/engine/vhs_fallback.rs`：palettegen + paletteuse 兩段合成（參數照 design §3）
2. `.part` → rename；fps 取 Script.fps（None→50）；非 `.gif` 目標 → 明確 bail!
3. ffmpeg stdout/stderr capture，失敗時把尾部輸出帶進錯誤訊息
4. `execute_script` 於 fallback 觸發後印辨識訊息（同 --max-size channel）
5. 單元測試：fps 解析、副檔名分派、（假 ffmpeg or 吃真序列的整合測試擇一）

## T3 — e2e 驗收（Hermes host 端執行，非 repo commit）

1. `VHS_BIN=/usr/bin/vhs`（0.12.0）`tapedeck run <smoke .roll>` → gif 存在＋ffprobe 有效
   ＋「已從 frames 合成」訊息
2. 預設 PATH（fork vhs）同 roll → gif 存在、無 fallback 訊息、`/tmp` 無 sink 殘留
3. webp 目標＋0.12.0 → 明確錯誤訊息（SCN-3）
4. 報告寫入 ticket 交付區
