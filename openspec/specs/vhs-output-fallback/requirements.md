# requirements.md — vhs-output-fallback

## 需求

### REQ-F1（frame sink）
tape codegen 末尾固定附加一行 `Output "<sink>/frames.png"`（帶引號）。sink 路徑每次執行
唯一（pid＋時間戳），與既有 Output、filmstrip Screenshot（output 旁 `frames/`）互不衝突。

### REQ-F2（缺失偵測）
vhs exit 0 後檢查目標輸出：不存在或 0 bytes → 觸發 fallback。非 0 exit 維持現行
`bail!`（誠實失敗，不救援）。

### REQ-F3（自力合成）
fallback 用 sink frames 目錄（`frame-cursor-%05d.png`，v0.12.0 與 HEAD 實測命名相同）
兩段 palette 合成目標 GIF：

```
ffmpeg -y -framerate <fps> -i <sink>/frame-cursor-%05d.png -vf palettegen <tmp>/pal.png
ffmpeg -y -framerate <fps> -i <sink>/frame-cursor-%05d.png -i <tmp>/pal.png -lavfi paletteuse <target>
```

- `<fps>` 取腳本 Set Framerate（未設＝vhs 預設 50）；`-framerate` 必須在 `-i` **之前**
- 先寫 `<target>.part`，成功後 rename 到位（半檔不落地）
- 目標副檔名非 `.gif` 時不合成，只報明確錯誤（引導改用 fork vhs）

### REQ-F4（訊息）
fallback 觸發時印辨識性訊息（與 `--max-size` 訊息同 channel）：偵測到 vhs 未產出輸出、
已從 frames 自行合成。正常路徑不得出現此訊息。

### REQ-F5（清理）
無論正常或 fallback 路徑，sink 暫存目錄用畢即刪（best-effort，不遮蔽主錯誤）。

## 情境（Scenarios）

- **SCN-1 健康路徑**：fork vhs → gif 正常產出、sink 未動用、暫存清除、無 fallback 訊息
- **SCN-2 問題路徑**：`VHS_BIN=/usr/bin/vhs` → gif 缺失 → 合成 → 有效 gif + REQ-F4 訊息
- **SCN-3 非 gif 目標**：output 為 `.webm`/`.webp` 且缺失 → 明確錯誤、不做半吊子合成

品質複核（尺寸/時長 vs tape 設定）屬 T3 e2e 驗收程序（ffprobe），非 runtime 檢查；
runtime 只信 ffmpeg exit code。
