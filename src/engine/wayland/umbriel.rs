//! UmbrielCompositor — umbriel（cybertron 自研 Wayland compositor）控制 adapter（compositor-ctl T2）
//!
//! 走 spawn `umbriel` CLI ＋ `--json` 解析（design 決策 1：v0 不直連 socket），零新依賴（REQ-7）。
//! move 兩步法（design 決策 2）：上游無 by-id move，先 `window-focus:<id>` 再
//! `window-move-to-workspace:<ws>`——副作用＝focus 變更，錄製目標本來就要 focus（REQ-6）。

use super::compositor::{Compositor, WindowGeometry};
use super::probe::CompositorError;
use anyhow::Result;

/// umbriel adapter 實體（方法實作於 T2 接線）
pub struct UmbrielCompositor;

impl Compositor for UmbrielCompositor {
    fn find_window_geometry(&self, _target: &str) -> Result<WindowGeometry> {
        Err(
            CompositorError::Ipc("UmbrielCompositor 方法實作於 compositor-ctl T2 接線".to_owned())
                .into(),
        )
    }

    fn window_on_output(&self, win: &WindowGeometry) -> Result<WindowGeometry> {
        Ok(win.clone())
    }

    fn move_to_workspace(&self, _target: &str, _workspace_name: &str) -> Result<()> {
        Err(
            CompositorError::Ipc("UmbrielCompositor 方法實作於 compositor-ctl T2 接線".to_owned())
                .into(),
        )
    }
}
