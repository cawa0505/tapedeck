use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::process::Command;

/// 視�窗�幾何座標 (Bounding Box)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl WindowGeometry {
    /// � 轉�換為 wf-recorder 相容的 `-g` �幾何字�串 (例如 "1900,20 1240x840")
    pub fn to_wf_recorder_arg(&self, padding: u32) -> String {
        let x = self.x - padding as i32;
        let y = self.y - padding as i32;
        let w = self.width + (padding * 2);
        let h = self.height + (padding * 2);
        format!("{},{} {}x{}", x, y, w, h)
    }
}

/// � 跨 Compositor � 抽象介面
pub trait Compositor {
    /// 根據 title 或 app_id �尋�找指定視�窗的�幾何座標
    fn find_window_geometry(&self, target: &str) -> Result<WindowGeometry>;
    /// 將指定視�窗移至�靜默背景 Workspace
    #[allow(dead_code)] // OQ-02 GUI 工作接線後使用
    fn move_to_workspace(&self, target: &str, workspace_name: &str) -> Result<()>;
}

// =========================================================================
// 1. Niri Compositor Implementation
// =========================================================================
pub struct NiriCompositor;

#[derive(Debug, Deserialize)]
struct NiriWindow {
    #[allow(dead_code)] // Niri 靜默移動 (move-window-to-workspace --window-id) 接線後使用
    id: u64,
    title: Option<String>,
    app_id: Option<String>,
    layout: NiriLayout,
}

#[derive(Debug, Deserialize)]
struct NiriLayout {
    /// 新版 (niri 26.x)：視窗在捲動佈局中的位置 + 實際像素尺寸
    pos_in_scrolling_layout: Option<[f64; 2]>,
    window_size: Option<[u32; 2]>,
    /// 舊版 (niri ≤25.x)：logical geometry（上游改版前結構）
    logical_geometry: Option<NiriGeometry>,
}

impl NiriLayout {
    /// 新版欄位優先，舊版 fallback — 相容 niri 上游 JSON 結構改版
    fn to_geometry(&self) -> Option<WindowGeometry> {
        if let (Some(pos), Some(size)) = (&self.pos_in_scrolling_layout, &self.window_size) {
            return Some(WindowGeometry {
                x: pos[0] as i32,
                y: pos[1] as i32,
                width: size[0],
                height: size[1],
            });
        }
        self.logical_geometry.as_ref().map(|g| WindowGeometry {
            x: g.x,
            y: g.y,
            width: g.width,
            height: g.height,
        })
    }
}

#[derive(Debug, Deserialize)]
struct NiriGeometry {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl Compositor for NiriCompositor {
    fn find_window_geometry(&self, target: &str) -> Result<WindowGeometry> {
        let output = Command::new("niri")
            .args(["msg", "--json", "windows"])
            .output()?;

        if !output.status.success() {
            return Err(anyhow!("niri msg �� 命令�執行失敗"));
        }

        let windows: Vec<NiriWindow> = serde_json::from_slice(&output.stdout)?;

        let matched = windows
            .into_iter()
            .find(|w| {
                w.app_id.as_deref() == Some(target)
                    || w.title.as_deref().is_some_and(|t| t.contains(target))
            })
            .ok_or_else(|| anyhow!("在 Niri 中�找不到符合 '{}' 的視�窗", target))?;

        matched.layout.to_geometry().ok_or_else(|| {
            anyhow!(
                "Niri 視窗 '{}' 缺少可用的 geometry 欄位（上游結構改版？）",
                target
            )
        })
    }

    fn move_to_workspace(&self, target: &str, workspace_name: &str) -> Result<()> {
        // Niri 可透過 action � 轉派 Workspace
        let status = Command::new("niri")
            .args(["msg", "action", "focus-window", "--app-id", target])
            .status()?;

        if status.success() {
            Command::new("niri")
                .args(["msg", "action", "move-window-to-workspace", workspace_name])
                .status()?;
        }
        Ok(())
    }
}

// =========================================================================
// 2. Sway Compositor Implementation
// =========================================================================
pub struct SwayCompositor;

#[derive(Debug, Deserialize)]
struct SwayNode {
    name: Option<String>,
    app_id: Option<String>,
    rect: SwayRect,
    nodes: Vec<SwayNode>,
    floating_nodes: Vec<SwayNode>,
}

#[derive(Debug, Deserialize)]
struct SwayRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl SwayCompositor {
    fn search_tree(node: &SwayNode, target: &str) -> Option<WindowGeometry> {
        let is_match = node.app_id.as_deref() == Some(target)
            || node.name.as_deref().is_some_and(|n| n.contains(target));

        if is_match && node.rect.width > 0 && node.rect.height > 0 {
            return Some(WindowGeometry {
                x: node.rect.x,
                y: node.rect.y,
                width: node.rect.width,
                height: node.rect.height,
            });
        }

        for child in node.nodes.iter().chain(node.floating_nodes.iter()) {
            if let Some(geo) = Self::search_tree(child, target) {
                return Some(geo);
            }
        }
        None
    }
}

impl Compositor for SwayCompositor {
    fn find_window_geometry(&self, target: &str) -> Result<WindowGeometry> {
        let output = Command::new("swaymsg")
            .args(["-t", "get_tree", "-r"])
            .output()?;

        if !output.status.success() {
            return Err(anyhow!("swaymsg �� 命令�執行失敗"));
        }

        let root: SwayNode = serde_json::from_slice(&output.stdout)?;

        Self::search_tree(&root, target)
            .ok_or_else(|| anyhow!("在 Sway 視�窗樹中�找不到符合 '{}' 的視�窗", target))
    }

    fn move_to_workspace(&self, target: &str, workspace_name: &str) -> Result<()> {
        let criteria = format!(
            "[app_id=\"{}\"] move container to workspace {}",
            target, workspace_name
        );
        Command::new("swaymsg").arg(criteria).status()?;
        Ok(())
    }
}

// =========================================================================
// 3. 自動�偵�測當前環境
// =========================================================================
pub fn detect_compositor() -> Result<Box<dyn Compositor>> {
    let xdg_desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_lowercase();
    let wayland_display = std::env::var("WAYLAND_DISPLAY")
        .unwrap_or_default()
        .to_lowercase();

    if xdg_desktop.contains("niri") || wayland_display.contains("niri") {
        Ok(Box::new(NiriCompositor))
    } else if xdg_desktop.contains("sway") || wayland_display.contains("sway") {
        Ok(Box::new(SwayCompositor))
    } else {
        // 退回 Try Niri � 預設
        Ok(Box::new(NiriCompositor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// niri 26.x 新版 layout 結構（上游改版後）
    #[test]
    fn parses_new_niri_layout() {
        let json = r#"{
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
        }"#;
        let w: NiriWindow = serde_json::from_str(json).unwrap();
        let g = w.layout.to_geometry().unwrap();
        assert_eq!((g.x, g.y, g.width, g.height), (1, 1, 1168, 1860));
    }

    /// 舊版 layout 結構（niri ≤25.x）：logical_geometry fallback
    #[test]
    fn parses_legacy_niri_layout() {
        let json = r#"{
            "id": 1,
            "title": "old",
            "app_id": "kitty",
            "layout": {
                "logical_geometry": { "x": 0, "y": 0, "width": 800, "height": 600 }
            }
        }"#;
        let w: NiriWindow = serde_json::from_str(json).unwrap();
        let g = w.layout.to_geometry().unwrap();
        assert_eq!((g.x, g.y, g.width, g.height), (0, 0, 800, 600));
    }
}
