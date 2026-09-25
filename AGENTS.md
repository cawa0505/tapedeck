# Tapedeck 專案規則

## 文件優先（Documentation First）— 強制

**任何討論中的規格，在實作前必須先完成 OpenSpec 規範文件。**

流程：
1. 規格有實作意圖時，先在 `openspec/specs/<change-set>/` 建立四份文件：
   - `proposal.md` — 動機、問題、成功標準
   - `requirements.md` — 功能性需求（REQ-x）、情境（SCN-x）
   - `design.md` — 技術設計、資料模型、元件分工
   - `tasks.md` — 實作任務分解（供後續執行）
2. 文件完成並經用戶確認後，才允許開始改程式碼。
3. 已定案規格不得跳過文件直接實作；實作時以文件為唯一依據（文件 = 需求來源）。

## 驗證要求

- 不可在沒有執行可用測試的情況下提交（`cargo test` / `cargo build`）
- 修改 > 5 個檔案前，先向用戶摘要影響範圍
- 不要臆測檔案路徑，先以 glob/grep 驗證

## 程式碼品質（Code Quality）

- 模組化：一個 rs 檔單一職責，超過 ~400 行必須拆分（engine/ 下已按職責分模組）
- 單檔長度：新檔案建議 ≤ 400 行；超過需說明拆分理由
- 風格：`cargo fmt` 與 `cargo clippy` 通過才可提交
- 可讀性：命名採專案慣例（非動詞前綴）、避免巢狀過深（>3 層需重構）、註釋只解釋「為什麼」
- 抽象克制：不預先抽象（單一實作不建 trait）、YAGNI；4 大 Resilience 原則的適配器屬例外

## 慣例

- .roll 是 tapedeck 專用格式：vhs 轉譯層 + tapedeck 自有自動化層（見 openspec）
- 依循 XDG Base Directory 規範（config: `~/.config/tapedeck/`、state: `~/.local/state/tapedeck/`、cache: `~/.cache/tapedeck/`）
- 對外部工具依賴（niri/swaymsg/wf-recorder/ffmpeg/vhs）採適配器模式，不寫死 CLI 參數

## 本地補充規範

- repo 根目錄若有 `secrets.md`（gitignored、僅存在於開發本機），視為補充規範，效力與本檔同等
- 本檔只放開發規範，不記錄環境／部署脈絡——此類資訊由開發環境端的查詢服務與 agent 記憶動態提供，寫進 repo 必然過時
