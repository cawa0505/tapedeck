# niri windows JSON 結構改版紀錄

> 來源：2026-08-08 Native e2e 除錯（D3）實測發現
> 影響元件：`src/engine/wayland/compositor.rs`（NiriCompositor::find_window_geometry → WaitWindow）

## 改版摘要

niri 26.04 把 `niri msg --json windows` 的視窗 `layout` 欄位結構改版，
**移除 `logical_geometry`**，改用捲動佈局座標 + 像素尺寸。

改版前 tapedeck 的 `NiriLayout` struct 強制要求 `logical_geometry` 欄位，
導致 serde 對**所有**視窗解析失敗（missing field）→ `find_window_geometry`
每次都回 Err → `WaitWindow` 對實際存在的視窗誤報「未出現」→ 8 秒後 bail。
**症狀是「視窗開起來但錄製流程沒進去」** — 與視窗聚焦、執行環境無關。

## 結構對照

### 舊版（niri ≤ 25.x）

```json
{
  "id": 1,
  "title": "kitty",
  "app_id": "kitty",
  "layout": {
    "logical_geometry": { "x": 0, "y": 0, "width": 800, "height": 600 }
  }
}
```

### 新版（niri 26.x）

```json
{
  "id": 149,
  "title": "ztest9",
  "app_id": "kitty",
  "layout": {
    "pos_in_scrolling_layout": [1, 1],
    "tile_size": [1172.0, 1864.0],
    "window_size": [1168, 1860],
    "tile_pos_in_workspace_view": null,
    "window_offset_in_tile": [2.0, 2.0]
  }
}
```

- `pos_in_scrolling_layout`: 視窗在捲動佈局中的 (x, y)，f64
- `window_size`: 實際像素尺寸 (width, height)，u32
- 舊版欄位 `logical_geometry` 已移除

## 修復方式

`NiriLayout` 改成 Option 雙相容 — 新版欄位優先、舊版 fallback：

```rust
struct NiriLayout {
    pos_in_scrolling_layout: Option<[f64; 2]>,  // niri 26.x
    window_size: Option<[u32; 2]>,              // niri 26.x
    logical_geometry: Option<NiriGeometry>,     // niri ≤25.x
}

impl NiriLayout {
    fn to_geometry(&self) -> Option<WindowGeometry> {
        if let (Some(pos), Some(size)) = (&self.pos_in_scrolling_layout, &self.window_size) {
            return Some(WindowGeometry { x: pos[0] as i32, y: pos[1] as i32, width: size[0], height: size[1] });
        }
        self.logical_geometry.as_ref().map(|g| WindowGeometry { x: g.x, y: g.y, width: g.width, height: g.height })
    }
}
```

測試：`compositor.rs` 兩個 mock 單元測試（`parses_new_niri_layout` /
`parses_legacy_niri_layout`）鎖定兩種結構，不依賴真實 niri。

## 教訓

1. **上游 JSON 結構改版是常態** — 外部工具（niri/swaymsg/wf-recorder/ffmpeg）
   的 CLI 參數與 JSON 結構可能無預告變更（見 AGENTS.md 開源邊界）。
2. **這是 12.2（CI Mock Payload）要防的事件類型** — Mock 樣板能讓改版
   在 CI 第一時間被抓到，而非 Native e2e 手動除錯時才發現。
3. **結構改版的修復原則是雙相容**（新版優先 + 舊版 fallback），不該
   直接刪掉舊欄位 — 使用者可能還跑舊版 compositor。
