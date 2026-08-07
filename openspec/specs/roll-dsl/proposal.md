# proposal.md — .roll DSL 定案

## 動機（Motivation）

tapedeck 的 .roll 是自有腳本格式，目前處於「三種方言並存」的未定案狀態：

1. `examples/test_tui.roll` — 舊寫法（`Title`/`Mode`/`Output`/`FPS`，parser 已支援）
2. `examples/tui_zago.roll` — 新寫法（`Set Engine Auto`/`Set Output`/`Key Down` 無計數、`Key Enter`/`Key Tab`/`Key q`，parser 不支援）
3. `examples/gui_demo.roll` — 新寫法（`Set Engine Native`/`WaitWindow`/`WindowSize`/`Padding`/`Shortcut`/`Optimize`，parser 不支援）

parser 只實作了 subset，且執行層（dispatcher）只做了「轉譯 .tape 給 vhs」這半件事，自有自動化層指令（ExecBefore/WaitWindow/Roll/Optimize 等）尚未由 tapedeck 執行。

## 問題（Problem）

- 語法未定案導致 parser 與 examples 互相矛盾，AI agent 無法可靠產生 .roll 腳本
- 雙層設計未落實：vhs 轉譯層與 tapedeck 自動化層混在一起，自有指令被當成註解略過
- 舊寫法與新寫法的關係未定義，遷移路徑不明

## 成功標準（Success Criteria）

1. **語法定案**：以 examples 的新寫法為正式語法；`test_tui.roll` 的舊寫法保留為相容別名（alias）
2. **parser 擴充**：能完整解析 tui_zago.roll 與 gui_demo.roll，舊寫法 test_tui.roll 繼續可解析
3. **雙層落實**：
   - vhs 轉譯層：`Type`/`Enter`/`Key`/`Sleep`/`MouseMove`/`Click` → .tape（vhs 可執行）
   - tapedeck 自動化層：`ExecBefore`/`WaitWindow`/`TargetWindow`/`WindowSize`/`Padding`/`Roll`/`Shortcut`/`Optimize` → dispatcher 執行
4. **驗證**：`tapedeck run --dry-run examples/*.roll` 全部成功解析並顯示正確後端；`examples/test_tui.roll` 實際錄製仍成功
