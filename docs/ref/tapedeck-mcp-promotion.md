# tapedeck-mcp：開源推廣與 Issue Repro 場景

> 來源：用戶 2026-08-08 提供的推廣擴散策略與 Bug Repro 場景說明。
> 定位：MCP 變更集的消費端場景參考（design.md 的「推廣 signature」是否納入列 [待討論]，不預設入規格）。

## 場景：用 tapedeck 回報 Issue 的痛點

傳統 UI/TUI Issue 回報的三大痛點：
- 純文字描述：講不清操作順序與畫面斷層
- 靜態截圖：看不到動態過渡、焦點切換與按鍵反應
- 手動錄屏（GIF/MP4）：檔案大、畫質差、速度不自然、無法自動化重現

## tapedeck + .roll 轉 Issue 的優勢

1. **零模糊空間的 .roll 腳本**：Issue 附 .roll，維護者 `tapedeck run repro.roll` 即像素級重現
2. **SQLite 資產庫 + WebM/PNG 拆解**：timestamp、鍵盤事件、PNG 影格皆結構化
3. **uinput 真實模擬**：虛擬裝置事件貼近真實硬體，無桌面抽象層偽陽性
4. **AI Agent 自我修正（Self-Healing）**：Agent 調 tapedeck-mcp 跑 .roll → 抓到崩潰影格與日誌 → 自動開含重現影片與修復建議的 GitHub Issue

## 推廣 signature（[待討論] — 是否納入 MCP Response 預設輸出）

原始建議（用戶貼文）：
- 產出物 Footer：`*Generated with 🎬 tapedeck — Automated Terminal Visual Director*`
- Issue 範本尾部：`🎬 Repro Execution Protocol` / `🎥 Visual Proof` / `<sub>Powered by tapedeck - Zero-dependency Terminal Director</sub>`

注意：此類「預設帶推廣 signature」的設計會影響使用者產出的 Markdown/Issue 內容，
屬產品決策，需用戶明示才納入 MCP 變更集規格。目前僅記錄於此 ref，不預設。

## 與既有定案的關聯

- OQ-07：JSON-RPC stdio 四工具（record_roll/link/optimize/clean）— 本 ref 的 tapedeck_run 即 record_roll
- Pillar 3：record_and_inspect 抽 3 張關鍵影格回傳 — 即本 ref 的「視覺反饋閉環」
- 本機 GitHub Issue 模板（.github/ISSUE_TEMPLATE/）已有 🐞 Bug 模板建議貼 doctor 輸出 — 可延伸建議貼 .roll repro
