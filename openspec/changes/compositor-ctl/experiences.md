# experiences.md — compositor-ctl 偵察紀錄

日期：2026-09-26（cybertron session）
環境：umbriel repo clone `~/Workspace/umbriel`（HEAD 810711b）、系統 `/usr/bin/umbriel`、arhat niri 26.04 已驗 ssh 可達且 socket `/run/user/1000/niri.wayland-1.2292.sock`。

## 修正昨日結論

native-compositor-probe 票時期判斷「umbriel 本質不可用（無 niri/sway IPC）」——**偵察後證實錯誤**。umbriel 有完整 JSON IPC，只是 socket 命名不同（`umbriel-<WAYLAND_DISPLAY>.sock`），probe 沒找它。

## umbriel IPC 全貌

- Socket：`$XDG_RUNTIME_DIR/umbriel-<wayland-socket-name>.sock`（實機：`/run/user/1000/umbriel-wayland-0.sock`）
- 協定：newline 一筆 JSON request `{"cmd": "...", "arg": "..."}`（上限 64KB）→ JSON reply。穩定 reply 之外，`{"cmd":"subscribe","events":[...]}` 轉事件流
- Compositor binary 本身就是 CLI：`umbriel windows --json`、`umbriel workspaces --json`、`umbriel msg <action>`、`umbriel subscribe <events>`
- CLI 需要 `XDG_RUNTIME_DIR`＋`WAYLAND_DISPLAY`（錄製 session 內天然存在，Hermes shell 需手動帶）

### cmd 清單（release build 恆可用）

| cmd | 說明 |
| --- | --- |
| windows `--json` | 視窗清單（含幾何！見下） |
| workspaces `--json` | workspace 清單＋layout |
| msg `<action>` | 觸發鍵盤動作（全套，見下） |
| subscribe | 事件流（theme/overview/keyboard_layout/windows/workspaces/submap） |
| submap / layers / color / tearing / keyboard-layouts | 唯讀查詢 |

### windows --json 欄位（實機驗證）

id（ext-foreign 識別碼）、app_id、title、pid、workspace（`<OUTPUT>:<index>` 格式）、x/y/w/h（tiled 給 layout slot、float 給實際位置）、floating、focused、active、urgent、scratchpad、xdg_tag、content_type、xwayland。

→ **find_window_geometry 幾乎免費**；window_on_output 由 workspace 欄位解碼 output。

### msg action 重點

- **by-id 可選點**：`window-focus:<window-id>`、`window-focus-warp:<window-id>`、`window-close:[<window-id>]`
- **其餘 move/size 多為 focused-window 語意**：`window-move-to-workspace:<ws>[/<output>]`、`window-move-to-output-*`、`window-set-width:<fraction>`…
- 「搬指定視窗到 workspace」目前需兩步：先 `window-focus:<id>` 再 `window-move-to-workspace`。副作用：改變 focus（對錄製場景通常是想要的行为，錄主角本來就要 focus）
- scratchpad 系列（toggle/restore）可拿來做「隱藏/現身」編排

### 測試專用 cmd（#ifdef UMBRIEL_TEST_IPC，release 被 gate 掉）

`output-create/destroy`（headless output）、`settle`（等所有 output 畫完 frame）、`clock-freeze/advance/resume`（凍結動畫時鐘逐格推進）。

→ 這組示範了上游的意圖：**umbriel 本來就想被腳本驅動做錄製/測試**。整包測試版 binary 或 un-gate 屬上游工程（umbriel repo 建議票），v0 不依賴。

## Wayland 標準協定面（wayland-info 實測）

umbriel 已暴露：`zwlr_foreign_toplevel_manager_v1`(v3)、`ext_foreign_toplevel_list_v1`、`ext_foreign_toplevel_image_capture_source_manager_v1`＋`ext_image_copy_capture_manager_v1`（窗級擷取！）、`ext_workspace_manager_v1`、`zwlr_virtual_pointer/virtual_keyboard`。

→ 標準協定可吃「列舉/窗級擷取」；「全域幾何+搬移」仍以各家 IPC 最完整（標協沒有搬移）。策略：**語意層不變，provider 層走各家 IPC 為主、標協為輔**。

## 對 tapedeck Compositor trait 的映射（umbriel）

| trait 方法 | umbriel 途徑 | 成本 |
| --- | --- | --- |
| find_window_geometry | `windows --json` 解析 | 白拿 |
| window_on_output | workspace 欄位解碼 | 白拿 |
| move_to_workspace | `window-focus:<id>` + `window-move-to-workspace:<ws>` | 兩步，副作用=focus |
| （v1+ 可加）close | `window-close:<id>` | 白拿 |
| （v1+ 可加）subscribe 事件流取代 200ms 輪詢 | `subscribe windows` | 白拿 |

v0 實作建議：直接 spawn `umbriel` CLI 取 `--json` stdout（std::process），**零新依賴、零 socket code**；之後要事件流才直連 socket。

## 能力 × 途徑矩陣（初版）

| 能力 | niri (arhat) | sway | umbriel (cybertron) |
| --- | --- | --- | --- |
| 視窗列舉+幾何 | `niri msg --json windows` ✅ | `swaymsg -t get_tree` ✅ | `umbriel windows --json` ✅ 實測 |
| 視窗所在 output | workspace id ✅ | tree 解碼 ✅ | workspace 欄位 ✅ 實測 |
| 搬視窗到 workspace | 有 action（by-id 參數待驗） | `[container_id] move to workspace ns` ✅ | 兩步（focus+move）✅ |
| 事件訂閱 | `niri msg event-stream` ✅ | subscribe ✅ | `umbriel subscribe` ✅ 審過原始碼 |
| 窗級擷取（錄製用） | ext_image_copy_capture 支援度**待驗**（arhat 實測待辦） | 待驗 | 協定已暴露 ✅ |

## lan-mouse-ipc 評估結論

- **語意不合**：它是「KVM 前端↔daemon」協定（Activate/Create/Position…），不是 compositor 控制；直接復用 = 硬套。
- **形狀可借**：cfg-gated per-OS backend crates（input-capture/input-emulation Linux/Win/macOS 並存）＋ `input-event` 跨 OS 事件抽象——這正是未來 Windows provider 的藍圖。到那一步時，鍵鼠注入直接參考/依賴 lan-mouse 家族，**這才是 lan-mouse 在本計畫的真實位置**。

## 風險與待辦

1. niri `ext_image_copy_capture` 支援度：影響 niri 上窗級擷取能否走通用路（arhat 實測）。
2. umbriel by-id move 缺口：上游加參數是小改動（ipc_commands.cpp 一處 dispatch + action spec），值得開 umbriel repo 票。
3. release binary 無 settle/clock-*：確定性逐格錄製在 umbriel 上先不可用；niri 也沒有，v0 用真時鐘＋硬編即可。
4. tapedeck probe 需認得 `umbriel-*.sock`（見 REQ）。
