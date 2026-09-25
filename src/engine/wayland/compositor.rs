use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::probe::CompositorError;
use super::umbriel::UmbrielCompositor;

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
    /// niri 的 scrolling 佈局座標 ≠ 輸出座標；sway/umbriel 無此問題（no-op）。
    fn window_on_output(&self, win: &WindowGeometry) -> Result<WindowGeometry>;
    /// 將指定視窗移至靜默背景 Workspace
    #[allow(dead_code)] // OQ-02 GUI 工作接線後使用
    fn move_to_workspace(&self, target: &str, workspace_name: &str) -> Result<()>;
}

// ── 錯誤分類輔助：IPC 命令失敗一律歸 CompositorError::Ipc 並附根因（REQ-2）──

/// 執行 compositor IPC 命令並檢查退出碼；失敗歸 `CompositorError::Ipc`
/// 並附根因（io error 描述或 stderr 尾段）
pub(super) fn checked_output(mut cmd: Command, ipc: &str) -> Result<Vec<u8>, CompositorError> {
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
// 3. 自動偵測當前環境（REQ-1：env 字串 → socket 探針 → 明確 Err，無 fallback）
// =========================================================================

/// compositor 種類：分類與實體化解開，`classify` 純函式才可注入測試
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositorKind {
    Niri,
    Sway,
    Umbriel,
}

impl fmt::Display for CompositorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Niri => write!(f, "Niri"),
            Self::Sway => write!(f, "Sway"),
            Self::Umbriel => write!(f, "Umbriel"),
        }
    }
}

/// doctor 檢查項用的偵測報告（REQ-3）：session 變數現值、socket、偵測結果
pub struct CompositorReport {
    pub xdg_desktop: Option<String>,
    pub wayland_display: Option<String>,
    pub runtime_dir: Option<PathBuf>,
    /// 探測到的 niri/sway/umbriel IPC socket 路徑
    pub socket: Option<PathBuf>,
    /// 偵測成功時的種類（與 `error` 恰其一）
    pub kind: Option<CompositorKind>,
    /// 偵測失敗原因（含 session 變數現值，供 SCN-2/SCN-3 歸因）
    pub error: Option<String>,
}

/// `umbriel` CLI 的 IPC socket（`$XDG_RUNTIME_DIR` 下）：
/// `umbriel-<wayland-socket-name>.sock`（樣式匹配容錯；多個時排序取第一）
fn umbriel_socket(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut names: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.file_name())
        .filter(|n| {
            let n = n.to_string_lossy();
            n.starts_with("umbriel-") && n.ends_with(".sock")
        })
        .collect();
    names.sort();
    names.first().map(|n| dir.join(n))
}

/// `niri msg` 的 IPC socket（`$XDG_RUNTIME_DIR` 下）：
/// 新版固定 `niri.sock`，舊版 `niri-<display>.sock`（樣式匹配容錯）
fn niri_socket(dir: &Path) -> Option<PathBuf> {
    let modern = dir.join("niri.sock");
    if modern.exists() {
        return Some(modern);
    }
    let entries = std::fs::read_dir(dir).ok()?;
    let mut names: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.file_name())
        .filter(|n| {
            let n = n.to_string_lossy();
            n.starts_with("niri-") && n.ends_with(".sock")
        })
        .collect();
    names.sort();
    names.first().map(|n| dir.join(n))
}

/// 目前使用者 uid（Linux：讀 /proc/self/status 的 Uid 行；None → 樣式匹配容錯）
fn current_uid() -> Option<u32> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("Uid:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

/// `swaymsg` 的 IPC socket：`$XDG_RUNTIME_DIR/sway-ipc.<uid>.sock`；
/// uid 解析失敗或檔名非標準時以 `sway-ipc.*.sock` 樣式匹配容錯
fn sway_socket(dir: &Path, uid: Option<u32>) -> Option<PathBuf> {
    if let Some(uid) = uid {
        let exact = dir.join(format!("sway-ipc.{uid}.sock"));
        if exact.exists() {
            return Some(exact);
        }
    }
    let entries = std::fs::read_dir(dir).ok()?;
    let mut names: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.file_name())
        .filter(|n| {
            let n = n.to_string_lossy();
            n.starts_with("sway-ipc.") && n.ends_with(".sock")
        })
        .collect();
    names.sort();
    names.first().map(|n| dir.join(n))
}

/// socket 探針（REQ-1 步驟 2）：niri 優先、其次 sway，皆未命中 → Err
fn probe_socket_kind(runtime_dir: &Path) -> Result<CompositorKind> {
    if niri_socket(runtime_dir).is_some() {
        return Ok(CompositorKind::Niri);
    }
    if sway_socket(runtime_dir, current_uid()).is_some() {
        return Ok(CompositorKind::Sway);
    }
    if umbriel_socket(runtime_dir).is_some() {
        return Ok(CompositorKind::Umbriel);
    }
    anyhow::bail!(
        "niri/sway/umbriel IPC socket 皆不存在於 {}",
        runtime_dir.display()
    )
}

/// 供 doctor 顯示：指定 runtime dir 下的 niri/sway socket 路徑
pub fn probe_socket_path(runtime_dir: &Path) -> Option<PathBuf> {
    niri_socket(runtime_dir)
        .or_else(|| sway_socket(runtime_dir, current_uid()))
        .or_else(|| umbriel_socket(runtime_dir))
}

/// 偵測純函式（REQ-1 順序：先 env 字串、後 socket 探針）——env 字串與
/// XDG_RUNTIME_DIR 皆可注入，單元測試不動 process env
fn classify(desktop: &str, display: &str, runtime_dir: Option<&Path>) -> Result<CompositorKind> {
    let desktop = desktop.to_lowercase();
    let display = display.to_lowercase();
    if desktop.contains("niri") || display.contains("niri") {
        return Ok(CompositorKind::Niri);
    }
    if desktop.contains("sway") || display.contains("sway") {
        return Ok(CompositorKind::Sway);
    }
    if desktop.contains("umbriel") || display.contains("umbriel") {
        return Ok(CompositorKind::Umbriel);
    }

    let runtime_dir = runtime_dir.ok_or_else(|| {
        anyhow!(
            "無法偵測 Wayland compositor（XDG_CURRENT_DESKTOP={desktop:?}、WAYLAND_DISPLAY={display:?}）：\
             XDG_RUNTIME_DIR 未設定，找不到 niri/sway IPC socket"
        )
    })?;
    probe_socket_kind(runtime_dir).map_err(|e| {
        anyhow!(
            "無法偵測 Wayland compositor（XDG_CURRENT_DESKTOP={desktop:?}、WAYLAND_DISPLAY={display:?}）：{e}"
        )
    })
}

/// 由 process env 讀參數後分類（引擎與 doctor 共用）
fn detect_kind() -> Result<CompositorKind> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    classify(&desktop, &display, runtime_dir.as_deref())
}

fn kind_to_compositor(kind: CompositorKind) -> Box<dyn Compositor> {
    match kind {
        CompositorKind::Niri => Box::new(NiriCompositor),
        CompositorKind::Sway => Box::new(SwayCompositor),
        CompositorKind::Umbriel => Box::new(UmbrielCompositor),
    }
}

/// 自動偵測當前環境（REQ-1）：env 字串 → socket 探針 → 明確 Err（無 fallback）
pub fn detect_compositor() -> Result<Box<dyn Compositor>> {
    detect_kind().map(kind_to_compositor)
}

/// doctor 用的偵測報告（REQ-3）：只收集現值，不 panic、不 crash
pub fn compositor_probe_report() -> CompositorReport {
    let kind = detect_kind();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    let socket = runtime_dir.as_deref().and_then(probe_socket_path);
    match kind {
        Ok(kind) => CompositorReport {
            xdg_desktop: std::env::var("XDG_CURRENT_DESKTOP").ok(),
            wayland_display: std::env::var("WAYLAND_DISPLAY").ok(),
            runtime_dir,
            socket,
            kind: Some(kind),
            error: None,
        },
        Err(e) => CompositorReport {
            xdg_desktop: std::env::var("XDG_CURRENT_DESKTOP").ok(),
            wayland_display: std::env::var("WAYLAND_DISPLAY").ok(),
            runtime_dir,
            socket,
            kind: None,
            error: Some(e.to_string()),
        },
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

    // ── T2：偵測（tempdir 假 socket，不依賴真 session、不動 process env）──

    /// 測試用臨時目錄（無 tempfile 相依；仿 dispatcher tests 慣例）
    fn temp_sockets_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tapedeck-t2-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn classify_env_string_takes_precedence() {
        // 字串命中即回傳，不做 socket 探針（runtime_dir 傳 None 驗證不觸探）
        assert_eq!(
            classify("Niri", "wayland-0", None).unwrap(),
            CompositorKind::Niri
        );
        assert_eq!(classify("sway", "", None).unwrap(), CompositorKind::Sway);
        // WAYLAND_DISPLAY 字串也可命中
        assert_eq!(
            classify("UTF-8", "sway-ipc", None).unwrap(),
            CompositorKind::Sway
        );
    }

    #[test]
    fn classify_env_miss_falls_back_to_socket_probe() {
        // 新版 niri.sock
        let dir = temp_sockets_dir("niri-modern");
        std::fs::write(dir.join("niri.sock"), b"").unwrap();
        assert_eq!(
            classify("GNOME", "wayland-0", Some(dir.as_path())).unwrap(),
            CompositorKind::Niri
        );
        let _ = std::fs::remove_dir_all(&dir);

        // 舊版 niri-<display>.sock
        let dir = temp_sockets_dir("niri-legacy");
        std::fs::write(dir.join("niri-wayland-0.sock"), b"").unwrap();
        assert_eq!(
            classify("", "wayland-1", Some(dir.as_path())).unwrap(),
            CompositorKind::Niri
        );
        let _ = std::fs::remove_dir_all(&dir);

        // sway-ipc.<uid>.sock
        let dir = temp_sockets_dir("sway");
        std::fs::write(dir.join("sway-ipc.1000.sock"), b"").unwrap();
        assert_eq!(
            classify("", "", Some(dir.as_path())).unwrap(),
            CompositorKind::Sway
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn classify_empty_dir_is_explicit_error_no_fallback() {
        // 誤判防呆（design §5）：空目錄 → 明確 Err，絕不落 niri fallback
        let dir = temp_sockets_dir("empty");
        let err = classify("unknown-de", "wayland-0", Some(dir.as_path())).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("無法偵測 Wayland compositor"), "{msg}");
        assert!(
            msg.contains("\"unknown-de\""),
            "須含 XDG_CURRENT_DESKTOP 現值：{msg}"
        );
        assert!(msg.contains("皆不存在"), "{msg}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn classify_without_runtime_dir_is_explicit_error() {
        // SCN-3：無任何 wayland session 變數/路徑 → 明確錯誤，不誤導
        let err = classify("", "wayland-0", None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("XDG_RUNTIME_DIR 未設定"), "{msg}");
        assert!(msg.contains("\"wayland-0\""), "{msg}");
    }

    #[test]
    fn socket_probe_prefers_niri_over_sway() {
        let dir = temp_sockets_dir("both");
        std::fs::write(dir.join("niri.sock"), b"").unwrap();
        std::fs::write(dir.join("sway-ipc.1000.sock"), b"").unwrap();
        assert_eq!(probe_socket_kind(&dir).unwrap(), CompositorKind::Niri);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sway_socket_style_match_tolerates_uid_mismatch() {
        let dir = temp_sockets_dir("sway-uid");
        std::fs::write(dir.join("sway-ipc.4242.sock"), b"").unwrap();
        let found = sway_socket(&dir, Some(1000)).unwrap();
        assert!(found.ends_with("sway-ipc.4242.sock"));
        // uid 未知（/proc 解析失敗）也能以樣式匹配找到
        assert!(sway_socket(&dir, None).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compositor_kind_display() {
        assert_eq!(CompositorKind::Niri.to_string(), "Niri");
        assert_eq!(CompositorKind::Sway.to_string(), "Sway");
        assert_eq!(CompositorKind::Umbriel.to_string(), "Umbriel");
    }

    #[test]
    fn compositor_report_kind_and_error_are_exclusive() {
        // 環境相依（開發機非 niri/sway session → error；niri/sway session → kind），
        // 只驗證恰一為真，且失敗訊息帶 session 變數現值
        let report = compositor_probe_report();
        match (&report.kind, &report.error) {
            (Some(_), None) => {}
            (None, Some(err)) => assert!(err.contains("XDG_CURRENT_DESKTOP"), "{err}"),
            other => panic!("kind/error 恰其一，得到 {other:?}"),
        }
    }

    // ── compositor-ctl T1：umbriel 偵測 ──

    #[test]
    fn classify_env_umbriel_takes_precedence() {
        // XDG_CURRENT_DESKTOP / WAYLAND_DISPLAY 字串命中即回傳，不做 socket 探針
        assert_eq!(
            classify("umbriel", "wayland-0", None).unwrap(),
            CompositorKind::Umbriel
        );
        assert_eq!(
            classify("GNOME", "umbriel-wayland-0", None).unwrap(),
            CompositorKind::Umbriel
        );
    }

    #[test]
    fn classify_socket_probe_recognizes_umbriel() {
        // 僅 umbriel socket 存在 → 正確分類（REQ-1）
        let dir = temp_sockets_dir("umbriel-only");
        std::fs::write(dir.join("umbriel-wayland-0.sock"), b"").unwrap();
        assert_eq!(
            classify("", "", Some(dir.as_path())).unwrap(),
            CompositorKind::Umbriel
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn socket_probe_prefers_niri_then_sway_then_umbriel() {
        // 三者混合 → niri 優先（design 決策 3）
        let dir = temp_sockets_dir("umbriel-mixed-all");
        std::fs::write(dir.join("niri.sock"), b"").unwrap();
        std::fs::write(dir.join("sway-ipc.1000.sock"), b"").unwrap();
        std::fs::write(dir.join("umbriel-wayland-0.sock"), b"").unwrap();
        assert_eq!(probe_socket_kind(&dir).unwrap(), CompositorKind::Niri);
        let _ = std::fs::remove_dir_all(&dir);

        // sway + umbriel 混合 → sway 優先
        let dir = temp_sockets_dir("umbriel-mixed-sway");
        std::fs::write(dir.join("sway-ipc.1000.sock"), b"").unwrap();
        std::fs::write(dir.join("umbriel-wayland-0.sock"), b"").unwrap();
        assert_eq!(probe_socket_kind(&dir).unwrap(), CompositorKind::Sway);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn umbriel_socket_style_match_sorted_first() {
        // 多個 umbriel socket → 排序取第一（樣式匹配容錯，比照 niri_socket）
        let dir = temp_sockets_dir("umbriel-multi");
        std::fs::write(dir.join("umbriel-wayland-2.sock"), b"").unwrap();
        std::fs::write(dir.join("umbriel-wayland-0.sock"), b"").unwrap();
        let found = umbriel_socket(&dir).unwrap();
        assert!(found.ends_with("umbriel-wayland-0.sock"));
        // 無關檔名不會誤命中
        std::fs::write(dir.join("umbrielish.txt"), b"").unwrap();
        assert!(umbriel_socket(&dir).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn probe_socket_path_includes_umbriel() {
        // doctor 用路徑探針也認得 umbriel（niri/sway 皆缺席時）
        let dir = temp_sockets_dir("umbriel-path");
        std::fs::write(dir.join("umbriel-wayland-0.sock"), b"").unwrap();
        let found = probe_socket_path(&dir);
        assert!(found.is_some());
        assert!(found.unwrap().ends_with("umbriel-wayland-0.sock"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── compositor-ctl T2：umbriel adapter ──
}
