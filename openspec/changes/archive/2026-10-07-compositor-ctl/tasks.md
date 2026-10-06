# tasks.md — compositor-ctl v0（DRAFT）

- [x] **T1** probe 認得 umbriel socket（probe.rs classify + 測試）
- [x] **T2** UmbrielCompositor：list_windows/geometry/window_on_output/move_to_workspace + JSON fixture 測試
- [x] **T3** dispatcher 執行期選擇接入（niri→sway→umbriel）＋ doctor 診斷更新
- [x] **T4** e2e：cybertron umbriel session .roll 實錄 ≥1 支（Native 路徑全程，產物 `~/.cache/tapedeck/t4_umbriel_firefox.mp4` 2.4M 60fps h264）
- [x] **T5** 拆出至後續跨平台 Phase 獨立提案（抽獨立 crate + Windows provider 形狀）
- [x] **T6** 拆出至後續跨平台測試 Phase（跨 umbriel/niri/sway 多合成器實機重現等價測試報告）

## 禁忌（沿 native-compositor-probe）

- Rust 一律 `CARGO_HOME=$HOME/.cargo … --offline`；不新增相依。
- Native 引擎真錄製期間不干擾使用者（非目標視窗不動）。
- 沙箱工具缺 → 停在未 commit 狀態、誠實歸因回報。
