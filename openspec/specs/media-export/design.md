# design.md — 靜態媒體壓製技術設計

## 1. 模組結構（src/media/）

```
src/media/
├── mod.rs        # 公開 API：optimize()、filmstrip()
├── ffmpeg.rs     # FfmpegAdapter trait + 實作（Resilience 原則 1）
├── optimize.rs   # palettegen 雙 Pass / libwebp 壓製
├── filmstrip.rs  # 影格抽樣 + hstack 拼接
└── timeline.rs   # 時間點來源：vhs 注入 / JSONL 日誌 / 均勻抽樣
```

- 每檔單一職責，≤400 行（AGENTS.md 模組化規則）
- 不新增 crate 依賴：ffmpeg 以 CLI 子程序呼叫（沿用現有適配器模式）

## 2. FfmpegAdapter

```rust
// src/media/ffmpeg.rs
pub trait FfmpegAdapter {
    /// 版本與能力探針（Resilience 原則 2）：確認 palettegen/libwebp 可用
    fn probe(&self) -> Result<MediaCapabilities>;
    /// pass1：生成調色盤
    fn palettegen(&self, input: &Path, fps: u32) -> Result<PathBuf>;
    /// pass2：套用調色盤輸出 GIF
    fn paletteuse(&self, input: &Path, palette: &Path, output: &Path, fps: u32) -> Result<()>;
    /// WebP 輸出
    fn to_webp(&self, input: &Path, output: &Path, quality: u8) -> Result<()>;
    /// 指定時間點抽單幀 PNG（ffmpeg -ss）
    fn extract_frame(&self, input: &Path, ts_ms: u64, out: &Path) -> Result<()>;
    /// 橫向拼接多張 PNG（hstack）
    fn hstack(&self, frames: &[PathBuf], output: &Path) -> Result<()>;
}

pub struct MediaCapabilities {
    pub has_palettegen: bool,
    pub has_libwebp: bool,
    pub ffmpeg_version: String,
}
```

- `probe()` 於 optimize/filmstrip 啟動時呼叫；缺能力 → 明確錯誤訊息（含安裝提示）
- Lenient JSON：ffmpeg 若以 `-print_format json` 輸出，serde `#[serde(ignore_unknown_fields)]`（Resilience 原則 3）

## 3. palettegen 雙 Pass（GIF）

```
pass1: ffmpeg -i in.webm -vf "fps=10,palettegen=max_colors=256" palette.png
pass2: ffmpeg -i in.webm -i palette.png -lavfi "fps=10 [x];[x][1:v] paletteuse=dither=bayer:bayer_scale=5" out.gif
```

- 中間產物 palette.png 落於 `~/.cache/tapedeck/`（暫存，非輸出）
- `max_colors=256` 固定；dither 參數集中於 optimize.rs 常數，未來可調

## 4. 時間點來源（timeline.rs）

```rust
pub struct TimelinePoint {
    pub ms: u64,      // 相對起錄 0 基準
    pub label: String, // "Click Left" / "Type \"hi\""
}
```

三來源，依可用性優先：

1. **vhs 後端**：轉譯層（dispatcher `script_to_tape_content`）於每個 `Click`/`Type` 後注入
   `Screenshot "frames/NN.png"` — vhs 在指令完成當下截圖，時間點最精確。
   需 vhs 錄製完成後保留 `frames/` 目錄（於輸出同目錄），filmstrip 直接讀取。
   `[待討論]`：Screenshot 檔名計數與 .roll 指令對應（NN = 操作點序號）。
2. **Native 後端**：dispatcher 執行 `Click`/`Type` 時 append JSONL：
   `{"ms": 2340, "command": "Click Left"}` → 起錄時間由 wf-recorder 啟動時刻對齊。
   filmstrip 讀取後以 `ffmpeg -ss <ms>` 抽幀。
3. **fallback**：無 .roll / 無日誌 → 依影片時長等間距抽 `--count` 張。

## 5. Filmstrip 拼接

- 抽樣：取前 `--count` 個操作點；超過時合併間距 <500ms 的相近點 [待討論]
- 拼接：`ffmpeg -i f1.png -i f2.png ... -filter_complex "hstack=N" out.png`
- 影格間距：`hstack` 前先對每幀 `pad`（預設 12px 白色間距）[待討論]
- 底部步驟標籤 drawtext：依賴 fontconfig，預設不啟用 [待討論]

## 6. CLI 整合（src/cli.rs）

```
tapedeck optimize <input> [--format gif|webp] [--quality N] [--fps N] [--output PATH] [--dry-run]
tapedeck filmstrip <input> [--roll script.roll] [--count N] [--output PATH] [--dry-run]
```

- roll-dsl `Optimize <codec>` 指令於錄製後呼叫同一 `media::optimize` 模組（REQ-4.3）

## 7. 檔案影響範圍

| 檔案 | 變更 |
|------|------|
| `src/media/*`（新） | 上述 5 檔 |
| `src/cli.rs` | 新增 optimize/filmstrip 子指令 |
| `src/engine/dispatcher.rs` | ① vhs 轉譯注入 Screenshot ② Native 執行 Click/Type 時寫 JSONL |
| `src/lib.rs` / `src/main.rs` | media 模組註冊 |
| `Cargo.toml` | 無新增依賴 |
