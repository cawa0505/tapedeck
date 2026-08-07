# requirements.md — 靜態媒體自動壓製

## REQ-1：Smart GIF/WebP Export

- **REQ-1.1**：`tapedeck optimize <input.webm>` 支援 `--format gif|webp`（預設 gif）
- **REQ-1.2**：GIF 輸出採 palettegen 雙 Pass（pass1 產 palette.png、pass2 套用），`dither=bayer:bayer_scale=5` 降雜訊
- **REQ-1.3**：WebP 輸出用 libwebp（`--quality` 可調，預設 80）
- **REQ-1.4**：`--fps` 抽幀率可調（預設 10），控制檔案大小
- **REQ-1.5**：ffmpeg 透過 `FfmpegAdapter` trait 呼叫（Resilience 原則 1 適配器）
- **REQ-1.6**：輸出依 XDG 規則（REQ-5），`--output` 覆寫

## REQ-2：Filmstrip Step Sheet

- **REQ-2.1**：`tapedeck filmstrip <input.webm> --roll <script.roll>` 產生操作步驟分解圖
- **REQ-2.2**：依 .roll 內 `Click`/`Type` 指令時間點抽 3~5 張代表性 PNG 影格（預設取前 5 個操作點，`--count` 可調 3~10）
- **REQ-2.3**：影格時間點來源（依可用性優先）：
  - vhs 後端：於 .tape 每個 `Click`/`Type` 後注入 `Screenshot "frames/NN.png"` → vhs 原生產出精確影格
  - Native 後端：dispatcher 執行 `Click`/`Type` 時記錄 (elapsed_ms, command) 至 JSONL 日誌，事後 ffmpeg seek 抽幀
  - 兩者皆無：等間距均勻抽樣（fallback）
- **REQ-2.4**：橫向拼接單一 PNG（ffmpeg `hstack`，不引 imagemagick）
- **REQ-2.5**：多於 `--count` 個操作點時，合併間距 <500ms 的相近點 [待討論]
- **REQ-2.6**：影格間距與底部步驟標籤（drawtext）[待討論] — 預設僅間距、不標籤
- **REQ-2.7**：輸出依 XDG 規則（REQ-5），`--output` 覆寫

## REQ-3：時間點日誌（Native 後端）

- **REQ-3.1**：dispatcher 於 Native 模式執行 `Click`/`Type` 時，以 wf-recorder 起錄為 0 基準記錄 elapsed_ms
- **REQ-3.2**：日誌格式 JSONL，路徑 `~/.local/state/tapedeck/`（與錄製檔同名 `.timeline.jsonl`）
- **REQ-3.3**：日誌不存在或損壞時，filmstrip 走均勻抽樣 fallback（不失敗）

## REQ-4：CLI 整合

- **REQ-4.1**：新增子指令 `optimize`、`filmstrip`
- **REQ-4.2**：兩者皆支援 `--dry-run`（顯示將執行的 ffmpeg 指令，不實際執行）
- **REQ-4.3**：與 roll-dsl 的 `Optimize` 指令關係：.roll 內 `Optimize <codec>` 於錄製後呼叫同一 optimize 模組（共用實作）

## REQ-5：XDG 規範

- **REQ-5.1**：輸出預設 `$XDG_CACHE_HOME/tapedeck/`（未設定則 `~/.cache/tapedeck/`）
- **REQ-5.2**：純 std 解析（XDG_CACHE_HOME / HOME），不新增 `dirs` crate（沿用 roll-dsl REQ-6 模式）

## SCN 情境

- **SCN-1**：錄完 `test_tui.webm` → `optimize --format gif` → 產出可貼 README 的 GIF
- **SCN-2**：GUI 教學錄製 → `filmstrip --roll gui_demo.roll` → 產出 5 張步驟分解圖貼 docs
- **SCN-3**：無 .roll / 無時間戳日誌 → 均勻抽樣 fallback，仍產出步驟圖
- **SCN-4**：`--dry-run` 顯示完整 ffmpeg 指令鏈（palettegen → paletteuse）

## 非目標（Non-Goals）

- 影片剪輯 / 轉場 / 字幕
- 資產引用追蹤（屬 P2 / OQ-04 AssetTracker）
- 批次重錄（屬 P2 Re-roll，`[待討論]`）
- mp4/h264 容器（OQ-09 定案：webm 唯一容器，GIF/WebP 為輸出格式例外）
