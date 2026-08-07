# proposal.md — 靜態媒體自動壓製（Smart GIF/WebP + Filmstrip）

## 動機（Motivation）

錄完 WebM/AV1 影片後，tapedeck 不應該只是把影片放著。內容工程工作檯的定位需要兩項自動化：

1. **README/Medium 嵌入**：WebM 體積大且部分平台不直接顯示，需要高品質、小體積的 GIF/WebP
2. **靜態圖文教學**：.roll 已記錄 `Click`/`Type` 操作點，這些時間點可用來自動產出「操作步驟分解圖」

目前兩者皆為手工流程（手動跑 ffmpeg palettegen 雙 Pass、手動截圖拼接），門檻高且耗時。

## 問題（Problem）

- 壓製 GIF/WebP 需手工調 `palettegen`/`paletteuse` 參數，一般使用者不會
- 教學步驟圖需人工截圖 + 拼圖，不精準且費時
- .roll 中的 `Click`/`Type` 時間點資訊被丟棄，未被利用
- 缺乏一致的輸出路徑規則（XDG）

## 成功標準（Success Criteria）

1. **Smart GIF/WebP**：`tapedeck optimize <input.webm>` 一鍵輸出高品質靜態媒體（palettegen 雙 Pass，零雜訊、小體積）
2. **Filmstrip Step Sheet**：`tapedeck filmstrip <input.webm> --roll <script.roll>` 依 `Click`/`Type` 時間點自動輸出 3~5 張橫向步驟分解圖
3. **時間點來源**：vhs 後端注入 `Screenshot`、Native 後端用 dispatch 時間戳日誌、無資訊時均勻抽樣
4. **XDG 合規**：輸出預設 `~/.cache/tapedeck/`，`--output` 可覆寫
5. **驗證**：`examples/test_tui.roll` 錄製 → optimize → filmstrip 端到端成功
