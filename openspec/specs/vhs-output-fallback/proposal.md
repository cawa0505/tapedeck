# proposal.md — vhs 輸出健壯性：frame sink fallback（vhs-output-fallback）

## 動機（Motivation）

vhs 0.12.0 存在 GIF 靜默失敗 bug：teardown 先 cancel context，`Render(ctx)` 拿死 context
開 ffmpeg → EXIT=0 但輸出檔永不落地（上游 87b2f37 / PR #788 已修，v0.12.1）。
本機 `/usr/bin/vhs` 仍是 0.12.0，且各機上游升級節奏不可控。依 user 2026-09-26 決策走雙軌：

1. **vhs 端**：自維護 fork（cawa0505/vhs，parent＝charmbracelet/vhs），隨 upstream main
   同步、自行 build 安裝（已部署 `~/.local/bin/vhs`，煙囪測試通過）。
2. **tapedeck 端**（本票）：錄製時加掛 frames sink，偵測目標輸出缺失就自力合成——
   任何版本的 vhs 都保證「tape 跑完＝輸出存在」。

## 問題（Problem）

- 0.12.0 的 Output 靜默失敗無任何錯誤訊息，缺檔只能事後人眼發現
- 現行 `record()` 只檢查 exit code：成功 ≠ 有產物

## 成功標準（Success Criteria）

1. 問題版 vhs（`VHS_BIN=/usr/bin/vhs`＝0.12.0）跑完 → 目標輸出一定存在且為有效 GIF
   （fallback 路徑）
2. 健康版 vhs（fork HEAD）→ 行為與今天完全一致（fallback 不觸發、暫存即清）
3. 既有測試全綠；`run` / `reroll` 對外行為不變
