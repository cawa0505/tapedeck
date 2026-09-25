# tasks.md — compositor-ctl v0（DRAFT）

- [ ] **T1** probe 認得 umbriel socket（probe.rs classify + 測試）
- [ ] **T2** UmbrielCompositor：list_windows/geometry/window_on_output/move_to_workspace + JSON fixture 測試
- [ ] **T3** dispatcher 執行期選擇接入（niri→sway→umbriel）＋ doctor 診斷更新
- [ ] **T4** e2e：cybertron umbriel session .roll 實錄 ≥1 支（Native 路徑全程）
- [ ] **T5**（後續票 placeholder）抽 crate + Windows provider 形狀提案
- [ ] **T6** 重現等價 e2e：同支 .roll 於 umbriel＋niri（sway 機名待補）實機播放，輸出行為等價報告（若炸 scope 可拆後續票）

## 禁忌（沿 native-compositor-probe）

- Rust 一律 `CARGO_HOME=$HOME/.cargo … --offline`；不新增相依。
- Native 引擎真錄製期間不干擾使用者（非目標視窗不動）。
- 沙箱工具缺 → 停在未 commit 狀態、誠實歸因回報。
