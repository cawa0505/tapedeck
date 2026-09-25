# design.md — vhs-output-fallback

## 1. 事實基礎（2026-09-26 實測，勿重查）

- v0.12.0 與 HEAD 皆允許 tape 內**多個 Output**；主 Output 照跑，frames sink 照樣搬出
  （fork HEAD 實測 multi.gif＋sink.png 兩者皆產）
- sink 產物為 frames 目錄 rename，內含 `frame-cursor-%05d.png` 序列（兩版命名一致）
- `Output` 路徑必帶引號（tapedeck codegen 本來就寫 `Output "{}"`，dispatcher.rs:143）
- vhs 缺檔 bug 是 EXIT=0 靜默失敗 → 唯一可靠偵測點＝**事後檔案存在性**
- 呼叫層已隔離：`execute_script`（dispatcher.rs:524）統一 `run`/`reroll` 入口；
  `record()` 內 `VHS_BIN` 可插拔 binary（dispatcher.rs:116）
- ffmpeg 自製配方已在 0.12.0 實測閉環（palettegen→paletteuse）

## 2. 資料流

```
script_to_tape_content (dispatcher.rs:136)
  └─ 附加行: Output "<std::env::temp_dir()/tapedeck-<pid>-<ts>-sink/frames.png>"
record()
  ├─ vhs exit != 0 → bail!（維持現行）
  └─ exit == 0 → sink_present()? → target_exists()?
       ├─ 存在且 >0 bytes → Ok（SCN-1，最後刪 sink）
       └─ 缺失 → fallback::synthesize(sink, target, fps)?（SCN-2/3）
execute_script() 捕獲 fallback 訊息 → 已有 stdout 印訊息慣例（--max-size 同款）
```

## 3. 元件分工

| 元件 | 檔案 | 職責 |
|---|---|---|
| sink codegen | `dispatcher.rs` `script_to_tape_content` | 附加 sink Output 行；sink 路徑由 `VhsEngine` 持有 |
| 存在性偵測 | `dispatcher.rs` `record()` 後段 | exit 0 後檢查 target；缺失才走 fallback |
| 自力合成 | 新檔 `src/engine/vhs_fallback.rs` | ffmpeg 兩段 palette 合成；`.part`→rename；非 gif 报錯 |
| fps 來源 | `Script.fps` | None → 50（vhs 預設） |
| 清理 | `record()` 收尾 | sink 目錄 `remove_dir_all` best-effort |

`vhs_fallback.rs` 預估 <150 行，符合單檔 ≤400 行規範；ffmpeg 參數集中該檔單一常數區。

## 4. 邊界與陷阱

- **filmstrip 衝突**：既有 Screenshot 用的 `frames/` 在 output 旁；sink 在 `temp_dir()` 下
  唯一命名，兩者無交集（勿重用 `frames/` 名稱）
- **MCP stdout 污染**：record 過程 stdout→null 已存在；fallback 的 ffmpeg 用
  `Stdio::null()`＋capture，合成訊息由 execute_script 統一印（同 --max-size 慣例）
- **`Output` 附加行不得破壞 `--dry-run`**：dry-run 時列印的 commands 數不含 sink 行
  （sink 屬 codegen 細節，非 Script.commands）
- **Native 引擎完全不碰**：sink 與 fallback 僅存在於 `VhsEngine`
- **冪等**：fallback 失敗（ffmpeg 崩）→ bail! 帶 sink 目錄殘留路徑入錯誤訊息（利於除錯），
  不 retry
