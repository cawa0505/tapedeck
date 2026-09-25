//! UmbrielCompositor — umbriel（cybertron 自研 Wayland compositor）控制 adapter（compositor-ctl T2）
//!
//! 走 spawn `umbriel` CLI ＋ `--json` 解析（design 決策 1：v0 不直連 socket），零新依賴（REQ-7）。
//! move 兩步法（design 決策 2）：上游無 by-id move，先 `window-focus:<id>` 再
//! `window-move-to-workspace:<ws>`——副作用＝focus 變更，錄製目標本來就要 focus（REQ-6）。

use anyhow::Result;
use serde::Deserialize;
use std::process::Command;

use super::compositor::{checked_output, Compositor, WindowGeometry};
use super::probe::CompositorError;

pub struct UmbrielCompositor;

/// `umbriel windows --json` 的單一視窗（2026-09-26 cybertron 實測樣本）：
/// id 是 hex 字串非數字；x/y/w/h 為扁平欄位非嵌套
#[derive(Debug, Deserialize)]
struct UmbrielWindow {
    id: String,
    #[allow(dead_code)] // 兩步法僅需 id；欄位保留供解析驗證與除錯
    app_id: Option<String>,
    #[allow(dead_code)]
    title: Option<String>,
    #[allow(dead_code)] // 形如 "HDMI-A-1:1"（output:name）
    workspace: Option<String>,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
}

/// 兩步法 move 的第一步失敗與第二步失敗須可區分（錯誤穿隧測試用）
enum UmbrielMoveStep {
    Focus,
    Move,
}

impl UmbrielMoveStep {
    /// step 標籤：錯誤訊息標明失敗在哪一步（第一步失敗直接上拋、第二步保留根因）
    fn label(&self) -> &'static str {
        match self {
            Self::Focus => "window-focus",
            Self::Move => "window-move-to-workspace",
        }
    }

    /// 組 `umbriel msg <action>` 指令
    fn command(&self, arg: &str) -> Result<Command, CompositorError> {
        let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_default();
        self.command_with(&display, &runtime_dir, arg)
    }

    /// 可注入環境變數版本（單元測試不動 process env）
    fn command_with(
        &self,
        display: &str,
        runtime_dir: &str,
        arg: &str,
    ) -> Result<Command, CompositorError> {
        umbriel_env_check(display, runtime_dir)?;
        let mut cmd = Command::new("umbriel");
        cmd.args(["msg", &format!("{}:{arg}", self.label())]);
        Ok(cmd)
    }
}

/// REQ-4：umbriel CLI 必須繼承 session 的 WAYLAND_DISPLAY ＋ XDG_RUNTIME_DIR，缺一即 Ipc
/// （實際繼承由 std::process::Command 預設行為完成，這裡只驗證呼叫環境已具備）
fn umbriel_env_check(display: &str, runtime_dir: &str) -> Result<(), CompositorError> {
    if display.is_empty() || runtime_dir.is_empty() {
        return Err(CompositorError::Ipc(
            "umbriel CLI 需要 WAYLAND_DISPLAY 與 XDG_RUNTIME_DIR 環境變數（REQ-4），目前缺其中之一"
                .to_owned(),
        ));
    }
    Ok(())
}

/// 從 `umbriel windows --json` 解析結果中找出目標視窗 id（兩步法第一步的輸入）
fn umbriel_window_id(windows: &[UmbrielWindow], target: &str) -> Option<String> {
    windows
        .iter()
        .find(|w| {
            w.app_id.as_deref() == Some(target)
                || w.title.as_deref().is_some_and(|t| t.contains(target))
        })
        .map(|w| w.id.clone())
}

/// 扁平幾何欄位 → WindowGeometry（tiled 的 x/y 是 workspace 內 layout slot 座標）
fn umbriel_geometry(w: &UmbrielWindow) -> WindowGeometry {
    WindowGeometry {
        x: w.x,
        y: w.y,
        width: w.w,
        height: w.h,
    }
}

/// spawn `umbriel windows --json` 並解析（list_windows；CLI 失敗/JSON 壞 → Ipc 帶根因）
fn umbriel_list_windows() -> Result<Vec<UmbrielWindow>, CompositorError> {
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_default();
    umbriel_env_check(&display, &runtime_dir)?;
    let mut cmd = Command::new("umbriel");
    cmd.args(["windows", "--json"]);
    let stdout = checked_output(cmd, "umbriel windows")?;
    serde_json::from_slice(&stdout)
        .map_err(|e| CompositorError::Ipc(format!("umbriel windows 輸出 JSON 解析失敗：{e}")))
}

impl Compositor for UmbrielCompositor {
    /// 唯讀：spawn `umbriel windows --json` 找到目標視窗後回傳扁平幾何
    fn find_window_geometry(&self, target: &str) -> Result<WindowGeometry> {
        let windows = umbriel_list_windows()?;
        let matched = windows
            .iter()
            .find(|w| {
                w.app_id.as_deref() == Some(target)
                    || w.title.as_deref().is_some_and(|t| t.contains(target))
            })
            .ok_or_else(|| {
                CompositorError::NotFound(format!("在 Umbriel 中找不到符合 '{target}' 的視窗"))
            })?;
        Ok(umbriel_geometry(matched))
    }

    /// 唯讀：umbriel 座標語意（tiled 的 x/y 是 workspace 內 layout slot 座標）。
    /// 判斷依據（ticket 實測樣本）：單輸出 `HDMI-A-1:1`、x=969/y=50 與該輸出全域座標一致，
    /// 為全局座標非純 slot 相對值 → 比照 sway no-op；多輸出位移出現時再補 clip。
    fn window_on_output(&self, win: &WindowGeometry) -> Result<WindowGeometry> {
        Ok(win.clone())
    }

    /// 有 focus 副作用：umbriel 上游無 by-id move，兩步法
    /// `window-focus:<id>` → `window-move-to-workspace:<ws>`（錄製目標本來就要 focus，REQ-6）。
    /// 第一步失敗直接 Ipc 上拋；第二步失敗保留根因。
    fn move_to_workspace(&self, target: &str, workspace_name: &str) -> Result<()> {
        let windows = umbriel_list_windows()?;
        let id = umbriel_window_id(&windows, target).ok_or_else(|| {
            CompositorError::NotFound(format!("在 Umbriel 中找不到符合 '{target}' 的視窗"))
        })?;
        Ok(umbriel_focus_then_move(
            &id,
            workspace_name,
            &umbriel_run_checked,
        )?)
    }
}

/// 兩步法編排（錯誤穿隧測試可注入 runner）：第一步 window-focus 失敗直接上拋，
/// 第二步 window-move-to-workspace 保留根因——每步各自的根因已含在 runner 回傳錯誤內
fn umbriel_focus_then_move(
    id: &str,
    workspace: &str,
    run: &dyn Fn(UmbrielMoveStep, &str) -> Result<(), CompositorError>,
) -> Result<(), CompositorError> {
    run(UmbrielMoveStep::Focus, id)?;
    run(UmbrielMoveStep::Move, workspace)
}

/// 實際執行器：組指令 → checked_output（exit code 檢查＋stderr 尾段慣例）
fn umbriel_run_checked(step: UmbrielMoveStep, arg: &str) -> Result<(), CompositorError> {
    let cmd = step.command(arg)?;
    checked_output(cmd, step.label())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `umbriel windows --json` 實測樣本（2026-09-26 cybertron，ticket fixture）：
    /// id 為 hex 字串、x/y/w/h 扁平、workspace 形如 "HDMI-A-1:1"
    const UMBRIEL_WINDOWS_FIXTURE: &str = r#"[
        {"active":false,"app_id":"firefox","content_type":"none","floating":false,
         "focused":true,"h":1014,"id":"c38f9a8d600dfab440b5afc3a0b99a44","pid":1801,
         "scratchpad":"","title":"甲 - YouTube — Mozilla Firefox","urgent":false,
         "w":935,"workspace":"HDMI-A-1:1","x":969,"xdg_tag":"","xwayland":false,"y":50}
    ]"#;

    #[test]
    fn parses_umbriel_windows_fixture() {
        // id 為 hex 字串非數字；x/y/w/h 扁平欄位非嵌套
        let windows: Vec<UmbrielWindow> = serde_json::from_str(UMBRIEL_WINDOWS_FIXTURE).unwrap();
        assert_eq!(windows.len(), 1);
        let w = &windows[0];
        assert_eq!(w.id, "c38f9a8d600dfab440b5afc3a0b99a44");
        assert_eq!(w.workspace.as_deref(), Some("HDMI-A-1:1"));
        let g = umbriel_geometry(w);
        assert_eq!(g.x, 969);
        assert_eq!(g.y, 50);
        assert_eq!(g.width, 935);
        assert_eq!(g.height, 1014);
    }

    #[test]
    fn umbriel_window_id_matches_app_id_or_title() {
        let windows: Vec<UmbrielWindow> = serde_json::from_str(UMBRIEL_WINDOWS_FIXTURE).unwrap();
        // app_id 精確匹配
        assert_eq!(
            umbriel_window_id(&windows, "firefox").as_deref(),
            Some("c38f9a8d600dfab440b5afc3a0b99a44")
        );
        // title 包含匹配
        assert!(umbriel_window_id(&windows, "YouTube").is_some());
        // 查無
        assert!(umbriel_window_id(&windows, "foot").is_none());
    }

    #[test]
    fn umbriel_window_id_skips_geometry_check() {
        // 幾何為 0 的視窗仍可取得 id（兩步法第一步不是幾何查詢）
        let json = r#"[{"id":"abc","app_id":"foot","title":null,"workspace":null,
                    "x":0,"y":0,"w":0,"h":0}]"#;
        let windows: Vec<UmbrielWindow> = serde_json::from_str(json).unwrap();
        assert_eq!(umbriel_window_id(&windows, "foot").as_deref(), Some("abc"));
    }

    #[test]
    fn umbriel_env_check_missing_vars_is_ipc() {
        // REQ-4：缺任一環境變數 → Ipc 且訊息明示環境變數問題
        let err = umbriel_env_check("", "/run/user/1000").unwrap_err();
        assert!(err.is_ipc());
        assert!(err.to_string().contains("WAYLAND_DISPLAY"), "{err}");
        let err = umbriel_env_check("wayland-0", "").unwrap_err();
        assert!(err.is_ipc());
        let err = umbriel_env_check("", "").unwrap_err();
        assert!(err.is_ipc());
        // 齊備 → Ok
        assert!(umbriel_env_check("wayland-0", "/run/user/1000").is_ok());
    }

    #[test]
    fn umbriel_step_command_shape_and_env_gate() {
        // 指令外型：umbriel msg <action>:<arg>（比照 niri msg 模型）
        let cmd = UmbrielMoveStep::Focus
            .command_with("wayland-0", "/run/user/1000", "c38f9a8d")
            .unwrap();
        assert_eq!(cmd.get_program(), "umbriel");
        assert_eq!(
            cmd.get_args().collect::<Vec<_>>(),
            vec!["msg", "window-focus:c38f9a8d"]
        );
        let cmd = UmbrielMoveStep::Move
            .command_with("wayland-0", "/run/user/1000", "3")
            .unwrap();
        assert_eq!(
            cmd.get_args().collect::<Vec<_>>(),
            vec!["msg", "window-move-to-workspace:3"]
        );
        // 環境缺漏 → 指令不該被組出來（REQ-4 gate 也在第一步之前擋下）
        let err = UmbrielMoveStep::Focus
            .command_with("", "/run/user/1000", "x")
            .unwrap_err();
        assert!(err.is_ipc());
    }

    #[test]
    fn umbriel_two_step_focus_failure_tunnels_first_step() {
        // 第一步 window-focus 失敗 → 直接上拋該步根因，第二步不得執行
        let calls = std::cell::RefCell::new(Vec::new());
        let run = |step: UmbrielMoveStep, arg: &str| -> Result<(), CompositorError> {
            calls
                .borrow_mut()
                .push((step.label().to_owned(), arg.to_owned()));
            if step.label() == "window-focus" {
                return Err(CompositorError::Ipc(
                    "window-focus 結束碼 1，stderr：(stderr 空)".to_owned(),
                ));
            }
            Ok(())
        };
        let err = umbriel_focus_then_move("abc123", "2", &run).unwrap_err();
        assert!(err.is_ipc());
        assert!(err.to_string().contains("window-focus"), "{err}");
        assert_eq!(
            *calls.borrow(),
            vec![("window-focus".to_owned(), "abc123".to_owned())]
        );
    }

    #[test]
    fn umbriel_two_step_move_failure_preserves_root_cause() {
        // 第二步 window-move-to-workspace 失敗 → 保留第二步根因（不被吞掉）
        let calls = std::cell::RefCell::new(Vec::new());
        let run = |step: UmbrielMoveStep, arg: &str| -> Result<(), CompositorError> {
            calls
                .borrow_mut()
                .push((step.label().to_owned(), arg.to_owned()));
            if step.label() == "window-move-to-workspace" {
                return Err(CompositorError::Ipc(
                    "window-move-to-workspace 結束碼 1，stderr：unknown workspace".to_owned(),
                ));
            }
            Ok(())
        };
        let err = umbriel_focus_then_move("abc123", "2", &run).unwrap_err();
        assert!(err.is_ipc());
        assert!(
            err.to_string().contains("window-move-to-workspace"),
            "{err}"
        );
        assert!(
            err.to_string().contains("unknown workspace"),
            "須含根因：{err}"
        );
        // 兩步都有執行、順序正確
        assert_eq!(
            *calls.borrow(),
            vec![
                ("window-focus".to_owned(), "abc123".to_owned()),
                ("window-move-to-workspace".to_owned(), "2".to_owned()),
            ]
        );
    }

    #[test]
    fn umbriel_two_step_success_calls_both_in_order() {
        let calls = std::cell::RefCell::new(Vec::new());
        let run = |step: UmbrielMoveStep, arg: &str| -> Result<(), CompositorError> {
            calls
                .borrow_mut()
                .push((step.label().to_owned(), arg.to_owned()));
            Ok(())
        };
        assert!(umbriel_focus_then_move("abc123", "2", &run).is_ok());
        assert_eq!(
            *calls.borrow(),
            vec![
                ("window-focus".to_owned(), "abc123".to_owned()),
                ("window-move-to-workspace".to_owned(), "2".to_owned()),
            ]
        );
    }
}
