# Spec Delta

## Purpose
定義 tapedeck 對 umbriel 合成器（cybertron 本機自研 Wayland compositor）的控制與適配契約：包含 socket 偵測歸類、視窗幾何查詢與工作區移動，使 Native 錄製引擎可精確擷取目標視窗。

## ADDED Requirements

### Requirement: Umbriel 合成器偵測與分類
`probe.rs` MUST 正確分類 `umbriel-<display>.sock` 為 Umbriel 合成器，且執行期選擇序為 niri → sway → umbriel。

#### Scenario: 偵測到 Umbriel Socket
- **WHEN** 系統環境中存在 `umbriel-wayland-0.sock` 且 `XDG_CURRENT_DESKTOP=umbriel`
- **THEN** tapedeck doctor 與 compositor probe SHALL 正確識別為 `CompositorKind::Umbriel`

### Requirement: Umbriel 視窗查詢與幾何適配
`UmbrielCompositor` MUST 實作 Compositor trait，透過 `umbriel windows --json` 取得視窗列表並解析其位置與尺寸。

#### Scenario: 視窗幾何取得
- **WHEN** 指定目標視窗 app_id（如 `firefox`）
- **THEN** `UmbrielCompositor::find_window_geometry` SHALL 回傳相符視窗的 `(x, y, width, height)` 幾何邊界

### Requirement: Umbriel 兩步法工作區移動
當執行視窗移動至指定工作區時，MUST 透過 focus-then-move 兩步法安全完成，且失敗時保留根因。

#### Scenario: 工作區移動順序
- **WHEN** 呼叫 `move_to_workspace(id, ws)`
- **THEN** SHALL 先執行 window-focus 再執行 window-move-to-workspace，任一步失敗均傳回明確 IPC 錯誤
