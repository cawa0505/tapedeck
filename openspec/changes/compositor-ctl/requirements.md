# requirements.md — compositor-ctl v0（DRAFT）

- **REQ-1** 偵測：`detect_compositor()` 認得 `$XDG_RUNTIME_DIR/umbriel-*.sock` → 回 Umbriel；查無任何已知 socket → 明確 Err（維持 NotFound 語意、快速失敗，不加 fallback 偽裝）。
- **REQ-2** UmbrielCompositor 必須實作 niri 版同意語：find_window_geometry、window_on_output、move_to_workspace；前兩者唯讀、後者有 focus 副作用需在 doc 註明。
- **REQ-3** 錯誤分類沿用 CompositorError::{Ipc,NotFound}：CLI 不存在／socket 不存在／JSON 解析失敗 → Ipc 帶根因；僅查無符合條件視窗 → NotFound。
- **REQ-4** 執行 CLI 必須帶 `WAYLAND_DISPLAY`＋`XDG_RUNTIME_DIR`（繼承當前 session）；任一缺失 → Ipc 錯誤並明示是環境變數問題。
- **REQ-5** e2e：umbriel 實 session 上跑一支 .roll，Native engine 全程走 UmbrielCompositor 完成等待視窗→錄製→落檔（gif 或 mp4）。
- **REQ-6** 不打擾條款不變：批次錄製不動使用者焦點以外視窗；move_to_workspace 兩步法造成的 focus 變更，必須是錄製目標本身。
- **REQ-7** 不引入新 crate 依賴；v1 抽取成獨立 crate 才考慮。
- **REQ-8** 重現等價（replay parity）：同一支 .roll 在同平台不同 Wayland compositor（niri/sway/umbriel）以 Native 引擎播放，視窗等待、焦點移轉、輸入注入、擷取時點行為等價。可攜性契約：.roll 僅允許語意操作（語意等待、語意定位），禁固定 sleep、禁 compositor 鍵綁假設、禁絕對座標編排；compositor 差異 workaround 一律收斂於 adapter 層。降級原則暫定 fail-loud：不支援即報錯停住＋輸出「哪一步、哪家不支援」報告；.roll 可對單步明示標記 best-effort（跳過＋計入差異報告）。〔2026-09-26 Hermes 建議值，待使用者定調〕
