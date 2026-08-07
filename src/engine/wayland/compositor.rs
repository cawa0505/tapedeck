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
    fn move_to_workspace(&self, target: &str, workspace_name: &str) -> Result<()>;
}

// =========================================================================
// 1. Niri Compositor Implementation
// =========================================================================
pub struct NiriCompositor;

#[derive(Debug, Deserialize)]
struct NiriWindow {
    id: u64,
    title: Option<String>,
    app_id: Option<String>,
    layout: NiriLayout,
}

#[derive(Debug, Deserialize)]
struct NiriLayout {
    logical_geometry: NiriGeometry,
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
                    || w.title.as_deref().map_or(false, |t| t.contains(target))
            })
            .ok_or_else(|| anyhow!("在 Niri 中�找不到符合 '{}' 的視�窗", target))?;

        Ok(WindowGeometry {
            x: matched.layout.logical_geometry.x,
            y: matched.layout.logical_geometry.y,
            width: matched.layout.logical_geometry.width,
            height: matched.layout.logical_geometry.height,
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
            || node.name.as_deref().map_or(false, |n| n.contains(target));

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
        let criteria = format!("[app_id=\"{}\"] move container to workspace {}", target, workspace_name);
        Command::new("swaymsg").arg(criteria).status()?;
        Ok(())
    }
}

// =========================================================================
// 3. 自動�偵�測當前環境
// =========================================================================
pub fn detect_compositor() -> Result<Box<dyn Compositor>> {
    let xdg_desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_lowercase();
    let wayland_display = std::env::var("WAYLAND_DISPLAY").unwrap_or_default().to_lowercase();

    if xdg_desktop.contains("niri") || wayland_display.contains("niri") {
        Ok(Box::new(NiriCompositor))
    } else if xdg_desktop.contains("sway") || wayland_display.contains("sway") {
        Ok(Box::new(SwayCompositor))
    } else {
        // 退回 Try Niri � 預設
        Ok(Box::new(NiriCompositor))
    }
}