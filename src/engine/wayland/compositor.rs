use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;

use super::probe::CompositorError;

/// 視窗幾何座標 (Bounding Box)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl WindowGeometry {
    ///  轉換為 wf-recorder 相容的 `-g` 幾何字串 (例如 "1900,20 1240x840")
    pub fn to_wf_recorder_arg(&self, padding: u32) -> String {
        let x = self.x - padding as i32;
        let y = self.y - padding as i32;
        let w = self.width + (padding * 2);
        let h = self.height + (padding * 2);
        format!("{},{} {}x{}", x, y, w, h)
    }
}

///  跨 Compositor  抽象介面
pub trait Compositor {
    /// 根據 title 或 app_id 尋找指定視窗的幾何座標
    fn find_window_geometry(&self, target: &str) -> Result<WindowGeometry>;
    /// 將視窗座標轉為 wf-recorder 可用的輸出座標（並 clip 到輸出交集）。
    /// niri 的 scrolling 佈局座標 ≠ 輸出座標；sway 無此問題（no-op）。
    fn window_on_output(&self, win: &WindowGeometry) -> Result<WindowGeometry>;
    /// 將指定視窗移至靜默背景 Workspace
    #[allow(dead_code)] // OQ-02 GUI 工作接線後使用
    fn move_to_workspace(&self, target: &str, workspace_name: &str) -> Result<()>;
}

// ── 錯誤分類輔助：IPC 命令失敗一律歸 CompositorError::Ipc 並附根因（REQ-2）──

/// 執行 compositor IPC 命令並檢查退出碼；失敗歸 `CompositorError::Ipc`
/// 並附根因（io error 描述或 stderr 尾段）
fn checked_output(mut cmd: Command, ipc: &str) -> Result<Vec<u8>, CompositorError> {
    let out = cmd
        .output()
        .map_err(|e| CompositorError::Ipc(format!("{ipc} 執行失敗：{e}")))?;
    if !out.status.success() {
        return Err(CompositorError::Ipc(format!(
            "{ipc} 結束碼 {}，stderr：{}",
            out.status,
            stderr_tail(&out.stderr)
        )));
    }
    Ok(out.stdout)
}

/// stderr 尾段（≤200 字元）：命令失敗時的根因載體（空 stderr 以佔位說明）
fn stderr_tail(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes).trim().to_owned();
    if s.is_empty() {
        return "(stderr 空)".to_owned();
    }
    const LIMIT: usize = 200;
    let count = s.chars().count();
    if count <= LIMIT {
        return s;
    }
    let tail: String = s.chars().skip(count - LIMIT).collect();
    format!("(截尾) {tail}")
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

/// niri msg --json outputs 的單一輸出（logical = scrolling 平面座標）
#[derive(Debug, Deserialize)]
struct NiriOutput {
    logical: NiriOutputLogical,
}

#[derive(Debug, Clone, Deserialize)]
struct NiriOutputLogical {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
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

impl NiriCompositor {
    /// 視窗中心點所在輸出的 logical 區塊（scrolling 平面座標）
    fn output_containing(&self, cx: i32, cy: i32) -> Result<NiriOutputLogical> {
        let mut cmd = Command::new("niri");
        cmd.args(["msg", "--json", "outputs"]);
        let stdout = checked_output(cmd, "niri msg")?;
        let outs: HashMap<String, NiriOutput> = serde_json::from_slice(&stdout)
            .map_err(|e| CompositorError::Ipc(format!("niri msg 輸出 JSON 解析失敗：{e}")))?;
        for o in outs.values() {
            let (ox, oy) = (o.logical.x, o.logical.y);
            let (ow, oh) = (o.logical.width as i32, o.logical.height as i32);
            if cx >= ox && cx < ox + ow && cy >= oy && cy < oy + oh {
                return Ok(o.logical.clone());
            }
        }
        Err(anyhow!(
            "視窗中心 ({cx},{cy}) 不在任何 niri 輸出的 logical 範圍內"
        ))
    }

    /// 視窗與輸出 logical 矩形取交集，座標轉為輸出相對
    fn clip_to_output(win: &WindowGeometry, out: &NiriOutputLogical) -> Result<WindowGeometry> {
        let (ox, oy) = (out.x, out.y);
        let (ow, oh) = (out.width as i32, out.height as i32);

        let x = win.x.max(ox);
        let y = win.y.max(oy);
        let right = (win.x + win.width as i32).min(ox + ow);
        let bottom = (win.y + win.height as i32).min(oy + oh);

        if right <= x || bottom <= y {
            return Err(anyhow!(
                "視窗與輸出 ({},{} {}x{}) 無交集",
                out.x,
                out.y,
                out.width,
                out.height
            ));
        }

        Ok(WindowGeometry {
            x: x - ox,
            y: y - oy,
            width: (right - x) as u32,
            height: (bottom - y) as u32,
        })
    }
}

impl Compositor for NiriCompositor {
    fn find_window_geometry(&self, target: &str) -> Result<WindowGeometry> {
        let mut cmd = Command::new("niri");
        cmd.args(["msg", "--json", "windows"]);
        let stdout = checked_output(cmd, "niri msg")?;

        // 輸出 JSON 解析失敗屬 niri 版本結構改版，重試無益 → 併 IPC 層
        let windows: Vec<NiriWindow> = serde_json::from_slice(&stdout)
            .map_err(|e| CompositorError::Ipc(format!("niri msg 輸出 JSON 解析失敗：{e}")))?;

        let matched = windows
            .into_iter()
            .find(|w| {
                w.app_id.as_deref() == Some(target)
                    || w.title.as_deref().is_some_and(|t| t.contains(target))
            })
            .ok_or_else(|| {
                CompositorError::NotFound(format!("在 Niri 中找不到符合 '{target}' 的視窗"))
            })?;

        Ok(matched.layout.to_geometry().ok_or_else(|| {
            CompositorError::NotFound(format!(
                "Niri 視窗 '{target}' 缺少可用的 geometry 欄位（上游結構改版？）"
            ))
        })?)
    }

    fn window_on_output(&self, win: &WindowGeometry) -> Result<WindowGeometry> {
        let cx = win.x + win.width as i32 / 2;
        let cy = win.y + win.height as i32 / 2;
        let out = self.output_containing(cx, cy)?;
        Self::clip_to_output(win, &out)
    }

    fn move_to_workspace(&self, target: &str, workspace_name: &str) -> Result<()> {
        // Niri 可透過 action  轉派 Workspace
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
        let mut cmd = Command::new("swaymsg");
        cmd.args(["-t", "get_tree", "-r"]);
        let stdout = checked_output(cmd, "swaymsg")?;

        // 輸出 JSON 解析失敗屬 sway 版本結構改版，重試無益 → 併 IPC 層
        let root: SwayNode = serde_json::from_slice(&stdout)
            .map_err(|e| CompositorError::Ipc(format!("swaymsg 輸出 JSON 解析失敗：{e}")))?;

        Ok(Self::search_tree(&root, target).ok_or_else(|| {
            CompositorError::NotFound(format!("在 Sway 視窗樹中找不到符合 '{target}' 的視窗"))
        })?)
    }

    fn window_on_output(&self, win: &WindowGeometry) -> Result<WindowGeometry> {
        // sway 的 rect 即輸出座標（無 scrolling 佈局），原樣返回
        Ok(win.clone())
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
// 3. 自動偵測當前環境
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
        // 未匹配 → niri fallback（T2 改為 socket 探針與明確錯誤）
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

    #[test]
    fn clip_to_output_inside() {
        // 視窗完全在輸出內 → 原座標（相對輸出 = 0,0 起）
        let out = NiriOutputLogical {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let win = WindowGeometry {
            x: 100,
            y: 50,
            width: 800,
            height: 600,
        };
        let g = NiriCompositor::clip_to_output(&win, &out).unwrap();
        assert_eq!((g.x, g.y, g.width, g.height), (100, 50, 800, 600));
    }

    #[test]
    fn clip_to_output_truncates_beyond_output() {
        // 視窗超出輸出右/下邊界 → clip 到輸出邊界
        let out = NiriOutputLogical {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let win = WindowGeometry {
            x: 1800,
            y: 1000,
            width: 500,
            height: 400,
        };
        let g = NiriCompositor::clip_to_output(&win, &out).unwrap();
        assert_eq!((g.x, g.y, g.width, g.height), (1800, 1000, 120, 80));
    }

    #[test]
    fn clip_to_output_translates_to_output_relative() {
        // 次輸出（DP-2 at 1920,0）+ 視窗超出左邊界 → 座標轉為輸出相對 + clip
        let out = NiriOutputLogical {
            x: 1920,
            y: 0,
            width: 1200,
            height: 1920,
        };
        let win = WindowGeometry {
            x: 1900,
            y: 100,
            width: 800,
            height: 600,
        };
        let g = NiriCompositor::clip_to_output(&win, &out).unwrap();
        // x: max(1900,1920)-1920=0; y: 100-0=100; w: min(2700,3120)-1920=780; h: 600
        assert_eq!((g.x, g.y, g.width, g.height), (0, 100, 780, 600));
    }

    #[test]
    fn clip_to_output_no_overlap() {
        let out = NiriOutputLogical {
            x: 1920,
            y: 0,
            width: 1200,
            height: 1920,
        };
        let win = WindowGeometry {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        }; // 在 DP-1
        assert!(NiriCompositor::clip_to_output(&win, &out).is_err());
    }

    // ── T1：錯誤分類（不跑真 IPC）──

    #[test]
    fn stderr_tail_handles_empty_and_long() {
        assert_eq!(stderr_tail(b""), "(stderr 空)");
        assert_eq!(stderr_tail(b"  \n "), "(stderr 空)");
        assert_eq!(stderr_tail(b"boom"), "boom");
        let long: String = "x".repeat(300);
        let tail = stderr_tail(long.as_bytes());
        assert!(tail.starts_with("(截尾) "), "{tail}");
        assert_eq!(tail.chars().count(), "(截尾) ".chars().count() + 200);
    }

    #[test]
    fn ipc_error_wraps_root_cause_message() {
        let err: anyhow::Error = CompositorError::Ipc(
            "niri msg 執行失敗：No such file or directory (os error 2)".to_owned(),
        )
        .into();
        let msg = format!("{err:#}");
        assert!(msg.contains("IPC 不可用"), "{msg}");
        assert!(msg.contains("os error 2"), "須含根因：{msg}");
    }

    #[test]
    fn not_found_error_is_retryable_layer() {
        let err = CompositorError::NotFound("在 Niri 中找不到符合 'x' 的視窗".to_owned());
        assert!(!err.is_ipc());
        assert!(err.to_string().starts_with("查無視窗"));
    }
}
