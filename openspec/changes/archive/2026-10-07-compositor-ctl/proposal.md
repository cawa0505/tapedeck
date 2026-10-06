# proposal.md — compositor-ctl（DRAFT，待使用者確認）

## 為什麼

Generic 控制介面的需求已由兩件事定錨：
1. tapedeck Native 引擎需要 Compositor trait 在每台機器都有活著的實作（niri/sway/umbriel 各機異質）。
2. 使用者明示終局含 Windows——跨 OS 的控制層必然要抽出具體 trait 邊界。
3. 使用者 2026-09 定調：tapedeck 定位可能從個人工具升級為顧問場景「重現操作」的工具——門檻＝同一平台錄、同一平台重現；Wayland 上跨 compositor 盡量等價重播。niri/sway/umbriel 三台即為此標準而裝（lan-mouse、VeloKVM 同屬此標準的產物）→ 重現等價成為本 trait 的核心驗收，見 REQ-8。

## 方案（兩段式）

**v0（本票，tapedeck repo）**：
- `probe.rs` 偵測加入 `umbriel-<display>.sock`（明確歸類，不自 fallback）。
- 新增 `UmbrielCompositor` adapter：實作既有 Compositor trait（find_window_geometry / window_on_output / move_to_workspace），走 spawn `umbriel` CLI + `--json` 解析，零新依賴。
- dispatcher 執行期選擇：niri → sway → umbriel（依偵測結果；明確錯誤不再回 NotFound 誤導）。

**v1（後續票，新 crate）**：
- 把語意層抽成獨立 Rust crate（暫名 compositor-ctl，命名待使用者定奪），providers：niri / sway / umbriel / (future) win32。
- Windows provider 形狀參考 lan-mouse input-emulation 的 cfg-gated backend 模式。

## Alternatives 考量過的

- **直接復用 lan-mouse-ipc**：否決——語意是 KVM 前端協定，硬套會污染兩邊。其架構僅於未來 Windows 供程時參考。
- **純 Wayland 標準協定**：否決為主路——標協無搬移、幾何保證不一，僅作輔助。
- **等 umbriel 上游走 niri-ipc 相容**：否決——等待成本低不了，且 umbriel 已有可用 IPC 缺的只是 adapter。

## 影響

- tapedeck：新增一 adapter 檔 + probe 變更 + dispatcher 選擇點；既有 Niri/Sway adapter 不動。
- umbriel 本機：無需任何改動（v0）。
- 驗收環境：cybertron（umbriel 實session）＋ arhat（niri）雙點。
