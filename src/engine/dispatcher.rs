use crate::cli::RunArgs;
use crate::engine::roll_parser::{ClickType, Engine, Script, ScriptCommand};
use crate::engine::wayland::compositor::{detect_compositor, Compositor, WindowGeometry};
use crate::engine::wayland::probe::CompositorError;
use crate::paths::resolve_output_path;
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};
use tokio::process::Command as TokioCommand;
use tokio::time::sleep;

// ─────────────────────────────────────────────
// 錄影引擎抽象層（OQ-03 定案）
// ─────────────────────────────────────────────

/// 引擎 lifecycle：prepare（ExecBefore）→ record（錄製）→ cleanup（ExecAfter/優化）
#[async_trait]
pub trait RecordingEngine {
    async fn prepare(&self, script: &Script) -> Result<()>;
    async fn record(&self, script: &Script) -> Result<()>;
    async fn cleanup(&self, script: &Script) -> Result<()>;
}

// ─────────────────────────────────────────────
// 共用 helper
// ─────────────────────────────────────────────

async fn run_exec_cmds(script: &Script, before: bool, fail_fast: bool) -> Result<()> {
    for cmd in &script.commands {
        let exec = match (before, cmd) {
            (true, ScriptCommand::ExecBefore(c)) => c,
            (false, ScriptCommand::ExecAfter(c)) => c,
            _ => continue,
        };
        // D2 定案：ExecBefore/ExecAfter 的目標常是常駐 GUI 應用
        // （kitty/foot/obsidian），.status() 等待退出會永遠阻塞。
        // 改 spawn() 背景啟動：只確認命令能啟動，不等待退出；
        // 視窗就緒由後續 WaitWindow 指令負責。
        // 用 std spawn 而非 tokio：tokio Command 預設 stdio 為 null，
        // 繼承 stdio 與互動 shell 手動啟動的環境一致（D3 診斷發現）。
        let spawn = Command::new("sh")
            .arg("-c")
            .arg(exec)
            .spawn()
            .with_context(|| format!("failed to start sh -c: {exec}"));
        if let Err(e) = spawn {
            if fail_fast {
                return Err(e);
            }
            eprintln!("警告：ExecAfter 啟動失敗：{e:#}");
        }
    }
    Ok(())
}

/// wf-recorder 收尾寬限：優雅訊號後最多等待此時間，逾時才升級 SIGKILL。
const RECORDER_SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// 錄製程序收尾：送 SIGINT → 等待 `grace` → 逾時升級 SIGKILL。
///
/// 為什麼不直接 SIGKILL：wf-recorder 收到 SIGKILL 無法 finalize MP4 容器
/// （moov atom 缺失），產物不可播（ffprobe 報 Invalid data found）；
/// 實測 SIGINT 與 SIGTERM 都會走正常結束路徑完檔，故首選 SIGINT，
/// SIGKILL 僅作為逾時保底避免程序殘留。
async fn finalize_recorder(
    child: &mut tokio::process::Child,
    grace: Duration,
) -> Result<std::process::ExitStatus> {
    // SIGINT 首選：優雅訊號讓 wf-recorder 完整寫出容器索引（moov atom）
    if let Some(pid) = child.id() {
        // SAFETY：kill 僅對 recorder 的 pid 送訊號；libc 已在依賴樹
        // （REQ-7 不新增相依），直接呼叫勝過 shelling out 到外部 kill。
        let rc = unsafe { libc::kill(pid as libc::pid_t, libc::SIGINT) };
        if rc != 0 {
            // ESRCH：child 恰好自行退出；wait 仍取得到 status，不視為錯誤
            eprintln!(
                "警告：送 SIGINT 給 recorder (pid {pid}) 失敗：{}（可能已自行退出）",
                std::io::Error::last_os_error()
            );
        }
    }
    match tokio::time::timeout(grace, child.wait()).await {
        Ok(status) => Ok(status?),
        Err(_) => {
            eprintln!(
                "警告：recorder 逾時 {grace:?} 未退出，升級 SIGKILL（產物可能缺 moov atom 而不可播）"
            );
            let _ = child.kill().await;
            Ok(child.wait().await?)
        }
    }
}

/// 從 commands 找出第一個相符指令
fn find_cmd(script: &Script, pred: fn(&ScriptCommand) -> bool) -> Option<&ScriptCommand> {
    script.commands.iter().find(|c| pred(c))
}

/// 操作時間點日誌：以腳本時序推算 → `state_dir/<stem>.timeline.jsonl`（T3）
fn write_timeline(script: &Script, output: &Path) -> Result<()> {
    use crate::media::timeline::{compute_timeline, write_jsonl};

    let points = compute_timeline(script);
    if points.is_empty() {
        return Ok(());
    }
    let stem = output
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".to_owned());
    let path = crate::paths::state_dir().join(format!("{stem}.timeline.jsonl"));
    write_jsonl(&path, &points)
}

// ─────────────────────────────────────────────
// vhs 後端（TUI）：.roll → .tape → vhs
// ─────────────────────────────────────────────

pub struct VhsEngine {
    output: PathBuf,
    /// frames sink 暫存目錄（vhs-output-fallback REQ-F1）：codegen 附加行與缺檔
    /// 偵測都依它而定
    sink: PathBuf,
}

impl VhsEngine {
    pub fn new(output: PathBuf) -> Self {
        Self {
            output,
            sink: sink_dir(),
        }
    }
}

/// 本次執行唯一的 frames sink 暫存目錄（REQ-F1）：`temp_dir()/tapedeck-<pid>-<millis>-sink`。
/// 在 temp_dir 下唯一命名，與既有 Output 及 filmstrip Screenshot 的 `frames/`
/// （output 旁）互不衝突。
fn sink_dir() -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("tapedeck-{}-{}-sink", std::process::id(), millis))
}

/// 目標輸出是否存在且非 0 bytes（REQ-F2：EXIT=0 ≠ 有產物，事後存在性是唯一可靠偵測點）
fn target_present(target: &Path) -> bool {
    std::fs::metadata(target)
        .map(|m| m.len() > 0)
        .unwrap_or(false)
}

#[async_trait]
impl RecordingEngine for VhsEngine {
    async fn prepare(&self, script: &Script) -> Result<()> {
        run_exec_cmds(script, true, true).await
    }

    async fn record(&self, script: &Script) -> Result<()> {
        let tape_content = script_to_tape_content(script, &self.output, &self.sink)?;

        // vhs 的 Screenshot 指令不自動建目錄 — 預先建立 shots_dir（filmstrip 來源 1）
        if let Some(parent) = self.output.parent() {
            std::fs::create_dir_all(parent.join("frames")).with_context(|| {
                format!("無法建立 frames 目錄: {}", parent.join("frames").display())
            })?;
        }

        // 建立暫存 .tape 檔案
        let tape_path = std::env::temp_dir().join(format!("tapedeck-{}.tape", std::process::id()));
        std::fs::write(&tape_path, tape_content)
            .with_context(|| format!("無法寫入暫存 .tape 檔案: {}", tape_path.display()))?;

        // 呼叫 vhs
        // stdout → null：vhs 進度輸出（Creating/Host your GIF）在 MCP stdio 模式會污染
        // JSON-RPC 協定串流（stdout 只能有 MCP 訊息）；stderr 保留給錯誤訊息。
        let executable = std::env::var("VHS_BIN").unwrap_or_else(|_| "vhs".to_owned());
        let status = TokioCommand::new(&executable)
            .arg(&tape_path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .status()
            .await
            .with_context(|| format!("failed to start {executable}; install VHS or set VHS_BIN"))?;

        // 清理暫存檔案
        let _ = std::fs::remove_file(&tape_path);

        if !status.success() {
            // vhs 誠實失敗（REQ-F2：非 0 exit 不救援）；sink best-effort 即清
            let _ = std::fs::remove_dir_all(&self.sink);
            bail!("VHS exited with {status}");
        }

        // REQ-F2：0.12.0 的 GIF 靜默失敗是 EXIT=0 且輸出檔不落地——exit code 靠不住，
        // 事後檔案存在性是唯一可靠偵測點。
        if target_present(&self.output) {
            // SCN-1：健康路徑，sink 未動用 → best-effort 即清（REQ-F5）
            let _ = std::fs::remove_dir_all(&self.sink);
            return Ok(());
        }

        // 目標缺失：先誠實失敗（自力合成由 T2 接手）
        let _ = std::fs::remove_dir_all(&self.sink);
        bail!("VHS exit 0 但未產生輸出：{}", self.output.display())
    }

    async fn cleanup(&self, script: &Script) -> Result<()> {
        run_exec_cmds(script, false, false).await
    }
}

/// 將 .roll 腳本轉換為 VHS 可理解的 .tape 內容（VHS DSL）
fn script_to_tape_content(script: &Script, output: &Path, sink: &Path) -> Result<String> {
    let mut s = String::new();

    writeln!(s, "Output \"{}\"", output.display())?;
    if let Some(fps) = script.fps {
        writeln!(s, "Set Framerate {}", fps)?;
    }
    if let Some(term) = &script.shell {
        writeln!(s, "Set Shell \"{}\"", term)?;
    }

    // filmstrip 來源 1：操作點後注入 Screenshot（絕對路徑，落 output 旁 frames/ 目錄）
    let shots_dir = output
        .parent()
        .map(|p| p.join("frames"))
        .unwrap_or_else(|| std::path::PathBuf::from("frames"));
    let mut shot_n = 0u32;
    let shot_line = |n: u32| {
        format!(
            "Screenshot \"{}\"",
            shots_dir.join(format!("{:03}.png", n)).display()
        )
    };

    for cmd in &script.commands {
        match cmd {
            ScriptCommand::Type(text) => {
                writeln!(s, "Type \"{}\"", text)?;
                shot_n += 1;
                writeln!(s, "{}", shot_line(shot_n))?;
            }
            ScriptCommand::Key(name, count) => {
                let is_single_char = name.chars().count() == 1
                    && !matches!(
                        name.as_str(),
                        "Down"
                            | "Up"
                            | "Enter"
                            | "Tab"
                            | "Left"
                            | "Right"
                            | "PageUp"
                            | "PageDown"
                            | "Space"
                            | "Backspace"
                            | "Delete"
                            | "Insert"
                            | "Escape"
                            | "Home"
                            | "End"
                    );
                if is_single_char {
                    // 單一字母 → Type "q"（vhs 無單鍵指令）
                    for _ in 0..*count {
                        writeln!(s, "Type \"{}\"", name)?;
                    }
                } else if *count > 1 {
                    writeln!(s, "{} {}", name, count)?;
                } else {
                    writeln!(s, "{}", name)?;
                }
                shot_n += 1;
                writeln!(s, "{}", shot_line(shot_n))?;
            }
            ScriptCommand::Sleep(ms) => writeln!(s, "Sleep {}ms", ms)?,
            ScriptCommand::MouseMove(x, y) => writeln!(s, "MouseMove {} {}", x, y)?,
            ScriptCommand::Click(t) => {
                let btn = match t {
                    ClickType::Left => "left",
                    ClickType::Right => "right",
                    ClickType::Middle => "middle",
                };
                writeln!(s, "MouseClick {}", btn)?;
                shot_n += 1;
                writeln!(s, "{}", shot_line(shot_n))?;
            }
            ScriptCommand::Roll(secs) => writeln!(s, "Sleep {}s", secs)?,
            // vhs 指令全集透寫（REQ-7.1）：原樣轉譯
            ScriptCommand::Vhs(line) => writeln!(s, "{}", line)?,
            // tapedeck 自動化層指令（VHS 無對應）直接略過
            _ => {}
        }
    }

    // REQ-F1：末尾固定附加 `Output "<sink>/frames.png"`（帶引號）。vhs 支援多個
    // Output（design §1 實測）：主 Output 照跑、frames 目錄照樣搬出。sink 屬
    // codegen 細節，不進 Script.commands，`--dry-run` 的 commands 數不受影響。
    writeln!(s, "Output \"{}\"", sink.join("frames.png").display())?;

    Ok(s)
}

// ─────────────────────────────────────────────
// Native 後端（GUI）：compositor + wf-recorder
// ─────────────────────────────────────────────

/// WaitWindow 輪詢（native-compositor-probe T1／REQ-2）：只有「查無視窗」
/// 可重試（200ms）；IPC 層錯誤立即失敗並保留根因，不得被輪詢吞成「視窗未出現」。
/// 逾時訊息維持現行結構，附最後一次查無視窗的原因。
fn wait_for_window(
    compositor: &dyn Compositor,
    target: &str,
    deadline: Instant,
) -> Result<WindowGeometry> {
    let mut last_missing;
    loop {
        match compositor.find_window_geometry(target) {
            Ok(g) => return Ok(g),
            Err(e) if is_ipc_error(&e) => return Err(e),
            Err(e) => {
                last_missing = e.to_string();
                if Instant::now() >= deadline {
                    return Err(anyhow!(
                        "WaitWindow 逾時：視窗「{target}」未出現（最後一次查詢：{last_missing}）\
                         \n提示：可用 `tapedeck run --dry-run` 檢查，或先手動開啟目標視窗"
                    ));
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

/// IPC 層錯誤判別：`CompositorError` 經 `?` 包裝後仍可在 error chain 辨識
fn is_ipc_error(err: &anyhow::Error) -> bool {
    err.chain()
        .filter_map(|cause| cause.downcast_ref::<CompositorError>())
        .any(CompositorError::is_ipc)
}

pub struct NativeEngine {
    output: PathBuf,
}

impl NativeEngine {
    pub fn new(output: PathBuf) -> Self {
        Self { output }
    }
}

#[async_trait]
impl RecordingEngine for NativeEngine {
    async fn prepare(&self, script: &Script) -> Result<()> {
        run_exec_cmds(script, true, true).await
    }

    async fn record(&self, script: &Script) -> Result<()> {
        // 取得目標視窗幾何；compositor（非 Send）僅在 block 內存活，取得後即 drop
        let geometry_arg = {
            let compositor = detect_compositor()?;

            // TargetWindow / WaitWindow 目標（預設空字串 → 互動選擇）
            let target = match find_cmd(script, |c| matches!(c, ScriptCommand::TargetWindow(_))) {
                Some(ScriptCommand::TargetWindow(name)) => name.as_str(),
                _ => match find_cmd(script, |c| matches!(c, ScriptCommand::WaitWindow(..))) {
                    Some(ScriptCommand::WaitWindow(name, _)) => name.as_str(),
                    _ => "",
                },
            };

            // WaitWindow：每 200ms 輪詢直到成功或逾時（預設 10s）
            // 同步 sleep（CLI 單執行緒場景可接受），避免非 Send 的 compositor 跨 await
            let timeout_ms = match find_cmd(script, |c| matches!(c, ScriptCommand::WaitWindow(..)))
            {
                Some(ScriptCommand::WaitWindow(_, ms)) => *ms,
                _ => 10_000,
            };
            let deadline = Instant::now() + Duration::from_millis(timeout_ms);
            let geo = wait_for_window(compositor.as_ref(), target, deadline)?;

            // Padding → 幾何外擴
            let padding = match find_cmd(script, |c| matches!(c, ScriptCommand::Padding(_))) {
                Some(ScriptCommand::Padding(p)) => *p,
                _ => 0,
            };
            // 視窗座標 → 輸出座標（niri scrolling 平面 ≠ 輸出；sway/umbriel no-op）
            let out_geo = compositor.window_on_output(&geo)?;
            out_geo.to_wf_recorder_arg(padding)
        };

        // WindowSize：目前僅記錄，不調整視窗（OQ-02 待實作 resize）
        if let Some(ScriptCommand::WindowSize(w, h)) =
            find_cmd(script, |c| matches!(c, ScriptCommand::WindowSize(..)))
        {
            eprintln!("警告：WindowSize {w}x{h} 僅記錄，不調整視窗大小（OQ-02 待實作）");
        }

        // wf-recorder + 操作序列（OQ-02 輸入注入）
        let executable = std::env::var("WF_RECORDER").unwrap_or_else(|_| "wf-recorder".to_owned());
        let mut child = TokioCommand::new(&executable)
            .arg("-g")
            .arg(&geometry_arg)
            .arg("-f")
            .arg(&self.output)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .with_context(|| {
                format!("failed to start {executable}; install wf-recorder or set WF_RECORDER")
            })?;

        // 操作時間點日誌（T3）：腳本時序推算，寫入 state_dir/<stem>.timeline.jsonl
        // （filmstrip 以 ms 抽幀；實際執行時序與推算可能略有出入）
        write_timeline(script, &self.output)?;

        // 錄製循環：依序執行操作指令（wtype 鍵盤；libei 滑鼠能力偵測）
        let input = crate::engine::input::InputBackend::detect().adapter();
        let roll_dur = match find_cmd(script, |c| matches!(c, ScriptCommand::Roll(_))) {
            Some(ScriptCommand::Roll(secs)) => Some(Duration::from_secs(*secs)),
            _ => None,
        };
        let started = Instant::now();
        for cmd in &script.commands {
            match cmd {
                ScriptCommand::Type(t) => input
                    .key_type(t)
                    .with_context(|| "Type 輸入失敗；可用 `tapedeck doctor` 檢查 wtype 是否安裝")?,
                ScriptCommand::Key(name, n) => input
                    .key_press(name, *n)
                    .with_context(|| "Key 按鍵失敗；可用 `tapedeck doctor` 檢查 wtype 是否安裝")?,
                ScriptCommand::Shortcut(combo) => input
                    .shortcut(combo)
                    .with_context(|| "Shortcut 失敗；可用 `tapedeck doctor` 檢查 wtype 是否安裝")?,
                // 滑鼠：libei 無注入器 → 警告略過（能力偵測設計）
                ScriptCommand::MouseMove(x, y) => {
                    if let Err(e) = input.mouse_move(*x, *y) {
                        eprintln!("警告：MouseMove 略過（{e}）");
                    }
                }
                ScriptCommand::Click(button) => {
                    if let Err(e) = input.mouse_click(*button) {
                        eprintln!("警告：Click 略過（{e}）");
                    }
                }
                ScriptCommand::Sleep(ms) => {
                    if let Some(roll) = roll_dur {
                        let remain = roll.saturating_sub(started.elapsed());
                        if remain.is_zero() {
                            break; // Roll 到期，強制結束
                        }
                        sleep(Duration::from_millis((*ms).min(remain.as_millis() as u64))).await;
                    } else {
                        sleep(Duration::from_millis(*ms)).await;
                    }
                }
                _ => {} // 其餘指令（ExecBefore/After、WaitWindow 等）已在別處處理
            }
        }
        // 尾段：Roll 剩餘時間（若有），否則操作後短尾段
        if let Some(roll) = roll_dur {
            let remain = roll.saturating_sub(started.elapsed());
            if !remain.is_zero() {
                sleep(remain).await;
            }
        } else {
            sleep(Duration::from_millis(500)).await;
        }
        // 收尾：SIGINT 優雅完檔（moov atom）→ 逾時 5s 才升級 SIGKILL 保底
        let status = finalize_recorder(&mut child, RECORDER_SHUTDOWN_GRACE).await?;

        if !status.success() {
            bail!("wf-recorder exited with {status}");
        }
        Ok(())
    }

    async fn cleanup(&self, script: &Script) -> Result<()> {
        run_exec_cmds(script, false, false).await?;

        // Optimize(codec, kv) → ffmpeg 轉換（如 AV1 vaapi → av1_vaapi）
        if let Some(ScriptCommand::Optimize(codec, kv)) =
            find_cmd(script, |c| matches!(c, ScriptCommand::Optimize(..)))
        {
            let encoder = kv
                .iter()
                .find(|(k, _)| k == "encoder")
                .map(|(_, v)| v.as_str())
                .unwrap_or(codec);
            let stem = self
                .output
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "output".to_owned());
            let ext = self
                .output
                .extension()
                .map(|e| e.to_string_lossy().into_owned())
                .unwrap_or_else(|| "webm".to_owned());
            let optimized = self
                .output
                .with_file_name(format!("{stem}_optimized.{ext}"));

            let status = TokioCommand::new("ffmpeg")
                .arg("-y")
                .arg("-i")
                .arg(&self.output)
                .arg("-c:v")
                .arg(encoder)
                .arg(&optimized)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::inherit())
                .status()
                .await
                .with_context(|| "failed to start ffmpeg; install ffmpeg")?;
            if !status.success() {
                eprintln!("警告：Optimize 失敗（exit {status}），保留原始檔案");
            } else {
                println!("優化完成：{}", optimized.display());
            }
        }
        Ok(())
    }
}

// ─────────────────────────────────────────────
// 引擎解析 + CLI 入口
// ─────────────────────────────────────────────

/// 解析 .roll 的引擎設定（REQ-4.3）：Auto → 依腳本意圖自動選擇
pub(crate) fn resolve_engine(script: &Script) -> Engine {
    match script.engine {
        Some(Engine::Vhs) => Engine::Vhs,
        Some(Engine::Native) => Engine::Native,
        Some(Engine::Auto) | None => {
            let gui_intent = script.commands.iter().any(|c| {
                matches!(
                    c,
                    ScriptCommand::WaitWindow(..)
                        | ScriptCommand::TargetWindow(_)
                        | ScriptCommand::WindowSize(..)
                        | ScriptCommand::Padding(_)
                        | ScriptCommand::Shortcut(_)
                        | ScriptCommand::MouseMove(..)
                        | ScriptCommand::Click(_)
                        | ScriptCommand::Optimize(..)
                )
            });
            if gui_intent {
                Engine::Native
            } else {
                Engine::Vhs
            }
        }
    }
}

/// 設定檔 [defaults].engine 字串 → Engine（未知值回 None，維持 Auto 意圖）
fn parse_engine_str(s: &str) -> Option<Engine> {
    match s.to_ascii_lowercase().as_str() {
        "vhs" => Some(Engine::Vhs),
        "native" => Some(Engine::Native),
        "auto" => Some(Engine::Auto),
        _ => None,
    }
}

/// CLI 入口：解析 .roll 腳本並依引擎分派
/// T4b：--fps 覆寫優先序（CLI > 腳本 > config defaults）
fn apply_fps_precedence(script_fps: &mut Option<u32>, cli_fps: Option<u32>, cfg_fps: Option<u32>) {
    if let Some(fps) = cli_fps {
        *script_fps = Some(fps);
    } else if script_fps.is_none() {
        *script_fps = cfg_fps;
    }
}

/// T4b：--gif|--webp 覆寫輸出格式（vhs 以副檔名決定格式）
fn apply_format_override(output: &mut PathBuf, gif: bool, webp: bool) {
    let ext = if gif {
        Some("gif")
    } else if webp {
        Some("webp")
    } else {
        None
    };
    if let Some(ext) = ext {
        output.set_extension(ext);
    }
}

/// 共用執行選項（reroll T1 前置，design §2）：`execute_script` 的統一參數。
/// reroll 一律走腳本預設（`RunOptions::default()`）；`output` 覆寫在 reroll 的
/// 唯一合法用途是指向暫存路徑（design §3）。fps 優先序由呼叫方先行合併。
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// 輸出路徑覆寫（CLI 覆寫語意：None → 腳本內 Output / XDG 預設解析）
    pub output: Option<PathBuf>,
    /// 錄後同格式壓縮上限（MB）
    pub max_size: Option<u32>,
    /// 輸出格式覆寫：.gif
    pub gif: bool,
    /// 輸出格式覆寫：.webp
    pub webp: bool,
    /// 只解析與列印，不錄製（dry-run 輸出格式維持 `run` 既有行為）
    pub dry_run: bool,
}

/// 引擎預設補全（REQ-6.5）：腳本未指定 engine 才套用設定檔 defaults.engine
fn apply_engine_default(script: &mut Script, cfg_engine: Option<&str>) {
    if script.engine.is_none() {
        script.engine = cfg_engine.and_then(parse_engine_str);
    }
}

/// 共用執行入口（reroll T1 前置，design §2）：接收已 parse 的腳本（config 預設
/// 與 fps 優先序已由呼叫方補全），完成引擎解析、輸出解析（gif/webp 覆寫）、
/// dry-run 列印、輸出目錄建立、prepare/record/cleanup 與 max-size 壓縮，
/// 回傳實際輸出路徑。reroll 與 `run` 皆由此進入，單支行為保證一致（REQ-2.1 零旁路）。
pub async fn execute_script(
    script_file: &Path,
    script: Script,
    opts: &RunOptions,
) -> Result<PathBuf> {
    let engine = resolve_engine(&script);
    let mut output = resolve_output_path(
        script.output.as_deref().unwrap_or("output.webm"),
        opts.output.as_deref(),
    )?;

    // T4b：--gif|--webp 覆寫輸出格式（vhs 以 Output 副檔名決定格式，見 ref/vhs-tape-format.md:9）
    apply_format_override(&mut output, opts.gif, opts.webp);

    if opts.dry_run {
        // REQ-5 + REQ-6.3：印出引擎/輸出（解析後絕對路徑）/fps/指令摘要
        println!(
            "dry-run: {} engine={:?} output={} fps={:?} commands={}",
            script_file.display(),
            engine,
            output.display(),
            script.fps,
            script.commands.len()
        );
        return Ok(output);
    }

    // REQ-6.2：錄製前建立輸出目錄
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("無法建立輸出路徑: {}", parent.display()))?;
    }

    let backend: Box<dyn RecordingEngine + Send> = match engine {
        Engine::Vhs => Box::new(VhsEngine::new(output.clone())),
        Engine::Native => Box::new(NativeEngine::new(output.clone())),
        Engine::Auto => unreachable!("resolve_engine 已解析 Auto"),
    };

    backend.prepare(&script).await?;
    backend.record(&script).await?;
    backend.cleanup(&script).await?;

    // T4b：--max-size 超過時同格式壓縮（vhs 無原生 MaxSize，錄後檢查）
    if let Some(max_mb) = opts.max_size {
        if let Some((before, after)) = crate::media::optimize::compress_to_fit(&output, max_mb)? {
            println!(
                "--max-size {max_mb}MB：輸出超過上限（{:.1}MB），已壓縮至 {:.1}MB",
                before as f64 / (1024.0 * 1024.0),
                after as f64 / (1024.0 * 1024.0)
            );
        }
    }
    Ok(output)
}

/// CLI 入口薄殼（T1）：parse → config 預補全 → 組 RunOptions → execute_script。
/// 對外行為逐項不變（--dry-run 格式、fps 優先序、gif/webp、max-size）。
pub async fn run(args: RunArgs) -> Result<()> {
    let cfg = crate::config::load()?;
    let mut script = crate::engine::roll_parser::parse_roll_script(&args.script_file)?;

    // REQ-6.5 [defaults]：腳本未指定才套用設定檔預設（engine）
    apply_engine_default(&mut script, cfg.defaults.engine.as_deref());
    // T4b：--fps 覆寫（優先序 CLI > 腳本 > config）
    apply_fps_precedence(&mut script.fps, args.fps, cfg.defaults.fps);

    execute_script(
        &args.script_file,
        script,
        &RunOptions {
            output: args.output,
            max_size: args.max_size,
            gif: args.gif,
            webp: args.webp,
            dry_run: args.dry_run,
        },
    )
    .await
    .map(|_| ())
}

/// 產生媒體連結語法 (zola/md/html)
pub fn media_link(media_file: &Path, format: &str) -> Result<String> {
    let name = media_file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let display = media_file.display();

    let link = match format {
        "md" | "markdown" => format!("[{}]({})", name, display),
        "html" => format!("<img src=\"{}\" alt=\"{}\" />", display, name),
        "zola" => format!("{{{{ figure(src=\"{}\", alt=\"{}\") }}}}", display, name),
        _ => bail!("不支援的格式 '{}'，支援：md, html, zola", format),
    };
    Ok(link)
}

// ─────────────────────────────────────────────
// 單元測試
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_script() -> Script {
        Script {
            title: None,
            engine: None,
            shell: None,
            output: Some("assets/demo.webm".to_owned()),
            fps: Some(15),
            commands: vec![
                ScriptCommand::Type("hello".to_owned()),
                ScriptCommand::Key("q".to_owned(), 1),
                ScriptCommand::Key("Down".to_owned(), 3),
                ScriptCommand::Sleep(500),
                ScriptCommand::MouseMove(100, 200),
                ScriptCommand::Click(ClickType::Left),
                ScriptCommand::Roll(2),
                ScriptCommand::Vhs("Set Theme \"Dracula\"".to_owned()),
                ScriptCommand::ExecBefore("echo before".to_owned()),
                ScriptCommand::ExecAfter("echo after".to_owned()),
            ],
        }
    }

    // ── VHS 轉譯正確性（不實際呼叫 vhs）──

    #[test]
    fn tape_translation_basic() {
        let s = sample_script();
        let out = Path::new("/tmp/out/demo.webm");
        let tape = script_to_tape_content(&s, out, Path::new("/tmp/tapedeck-test-sink")).unwrap();

        assert!(tape.contains("Output \"/tmp/out/demo.webm\""));
        assert!(tape.contains("Set Framerate 15"));
        assert!(tape.contains("Type \"hello\""));
        assert!(tape.contains("Type \"q\""));
        assert!(tape.contains("Down 3"));
        assert!(tape.contains("Sleep 500ms"));
        assert!(tape.contains("MouseMove 100 200"));
        assert!(tape.contains("MouseClick left"));
        assert!(tape.contains("Sleep 2s"));
        assert!(tape.contains("Set Theme \"Dracula\""));
        // 自動化層指令不進 .tape
        assert!(!tape.contains("ExecBefore"));
        assert!(!tape.contains("ExecAfter"));
        assert!(!tape.contains("assets/demo.webm"));
    }

    #[test]
    fn tape_translation_key_count() {
        let s = Script {
            commands: vec![ScriptCommand::Key("Enter".to_owned(), 1)],
            ..sample_script()
        };
        let tape = script_to_tape_content(
            &s,
            Path::new("/tmp/o.webm"),
            Path::new("/tmp/tapedeck-test-sink"),
        )
        .unwrap();
        assert!(tape.contains("Enter"));
    }

    #[test]
    fn tape_translation_named_key_not_single_char() {
        let s = Script {
            commands: vec![ScriptCommand::Key("Enter".to_owned(), 2)],
            ..sample_script()
        };
        let tape = script_to_tape_content(
            &s,
            Path::new("/tmp/o.webm"),
            Path::new("/tmp/tapedeck-test-sink"),
        )
        .unwrap();
        assert!(tape.contains("Enter 2"));
    }

    // ── resolve_engine（REQ-4.3 Auto）──

    #[test]
    fn engine_auto_tui_script_goes_vhs() {
        let s = Script {
            engine: Some(Engine::Auto),
            commands: vec![ScriptCommand::Type("hi".to_owned())],
            ..sample_script()
        };
        assert_eq!(resolve_engine(&s), Engine::Vhs);
    }

    #[test]
    fn engine_auto_gui_script_goes_native() {
        let s = Script {
            engine: Some(Engine::Auto),
            commands: vec![ScriptCommand::WaitWindow("Obsidian".to_owned(), 10_000)],
            ..sample_script()
        };
        assert_eq!(resolve_engine(&s), Engine::Native);
    }

    #[test]
    fn engine_explicit_wins() {
        let s = Script {
            engine: Some(Engine::Native),
            commands: vec![ScriptCommand::Type("hi".to_owned())],
            ..sample_script()
        };
        assert_eq!(resolve_engine(&s), Engine::Native);
    }

    #[test]
    fn fps_cli_overrides_script_and_config() {
        let mut fps = Some(15);
        apply_fps_precedence(&mut fps, Some(30), Some(60));
        assert_eq!(fps, Some(30)); // CLI 最大優先
    }

    #[test]
    fn fps_config_fills_when_script_unset() {
        let mut fps = None;
        apply_fps_precedence(&mut fps, None, Some(60));
        assert_eq!(fps, Some(60)); // config 只填腳本未指定時
    }

    #[test]
    fn fps_script_kept_when_no_cli() {
        let mut fps = Some(15);
        apply_fps_precedence(&mut fps, None, Some(60));
        assert_eq!(fps, Some(15)); // 腳本優先於 config
    }

    #[test]
    fn format_gif_overrides_extension() {
        let mut out = PathBuf::from("/cache/demo.webm");
        apply_format_override(&mut out, true, false);
        assert_eq!(out, PathBuf::from("/cache/demo.gif"));
    }

    #[test]
    fn format_webp_overrides_extension() {
        let mut out = PathBuf::from("demo.webm");
        apply_format_override(&mut out, false, true);
        assert_eq!(out, PathBuf::from("demo.webp"));
    }

    #[test]
    fn format_none_keeps_extension() {
        let mut out = PathBuf::from("demo.webm");
        apply_format_override(&mut out, false, false);
        assert_eq!(out, PathBuf::from("demo.webm"));
    }

    // ── vhs-output-fallback T1：sink codegen ＋ 存在性偵測 ──

    #[test]
    fn tape_appends_sink_output_line_at_end() {
        let s = sample_script();
        let sink = Path::new("/tmp/tapedeck-42-1234-sink");
        let tape = script_to_tape_content(&s, Path::new("/tmp/out/demo.webm"), sink).unwrap();

        let sink_line = format!("Output \"{}/frames.png\"", sink.display());
        assert!(tape.contains(&sink_line));
        // sink 行固定在末尾（codegen 附加、帶引號）
        assert_eq!(tape.trim_end().lines().last(), Some(sink_line.as_str()));
        // 主 Output 行（首行）不受影響
        assert_eq!(tape.lines().next(), Some("Output \"/tmp/out/demo.webm\""));
    }

    #[test]
    fn sink_dir_unique_across_engine_instances() {
        let a = VhsEngine::new(PathBuf::from("/tmp/a.gif"));
        std::thread::sleep(Duration::from_millis(5));
        let b = VhsEngine::new(PathBuf::from("/tmp/b.gif"));
        assert_ne!(a.sink, b.sink);

        let name = a.sink.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with(&format!("tapedeck-{}-", std::process::id())));
        assert!(name.ends_with("-sink"));
    }

    #[test]
    fn codegen_never_touches_script_commands() {
        let s = sample_script();
        let before = s.commands.clone();
        let tape =
            script_to_tape_content(&s, Path::new("/tmp/o.gif"), Path::new("/tmp/sink-t1")).unwrap();
        assert_eq!(s.commands, before);
        assert_eq!(s.commands.len(), 10);
        // sink 屬 codegen 細節：Script.commands 不含 sink，tape 內容才含
        assert!(!before
            .iter()
            .any(|c| format!("{c:?}").contains("frames.png")));
        assert!(tape.contains("frames.png"));
    }

    #[test]
    fn target_present_requires_nonempty_file() {
        let dir = std::env::temp_dir().join(format!(
            "tapedeck-t1-target-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("empty.gif"), b"").unwrap();
        std::fs::write(dir.join("full.gif"), b"GIF89a").unwrap();

        assert!(target_present(&dir.join("full.gif")));
        // 0 bytes 視同未產出（REQ-F2）
        assert!(!target_present(&dir.join("empty.gif")));
        assert!(!target_present(&dir.join("missing.gif")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── native-compositor-probe T1:WaitWindow 錯誤分類（mock，不跑真 IPC）──

    /// stub：IPC 正常但永遠查無視窗
    struct NeverFound;

    impl Compositor for NeverFound {
        fn find_window_geometry(&self, target: &str) -> Result<WindowGeometry> {
            Err(CompositorError::NotFound(format!("在 Niri 中找不到符合 '{target}' 的視窗")).into())
        }
        fn window_on_output(&self, win: &WindowGeometry) -> Result<WindowGeometry> {
            Ok(win.clone())
        }
        fn move_to_workspace(&self, _target: &str, _workspace: &str) -> Result<()> {
            Ok(())
        }
    }

    /// stub：IPC 掛掉（模擬 socket 連不上／命令失敗）
    struct IpcDead;

    impl Compositor for IpcDead {
        fn find_window_geometry(&self, _target: &str) -> Result<WindowGeometry> {
            Err(CompositorError::Ipc(
                "niri msg 執行失敗：Connection refused (os error 111)".to_owned(),
            )
            .into())
        }
        fn window_on_output(&self, win: &WindowGeometry) -> Result<WindowGeometry> {
            Ok(win.clone())
        }
        fn move_to_workspace(&self, _target: &str, _workspace: &str) -> Result<()> {
            Ok(())
        }
    }

    /// stub：前兩次查無視窗、第三次成功（驗證 NotFound 會重試）
    struct FlakyFound(std::sync::atomic::AtomicUsize);

    impl Compositor for FlakyFound {
        fn find_window_geometry(&self, _target: &str) -> Result<WindowGeometry> {
            use std::sync::atomic::Ordering;
            let n = self.0.fetch_add(1, Ordering::SeqCst);
            if n < 2 {
                return Err(CompositorError::NotFound("視窗還沒開".to_owned()).into());
            }
            Ok(WindowGeometry {
                x: 1,
                y: 2,
                width: 10,
                height: 20,
            })
        }
        fn window_on_output(&self, win: &WindowGeometry) -> Result<WindowGeometry> {
            Ok(win.clone())
        }
        fn move_to_workspace(&self, _target: &str, _workspace: &str) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn wait_window_ipc_error_fails_fast_without_retry() {
        let deadline = Instant::now() + Duration::from_secs(30);
        let started = Instant::now();
        let err = wait_for_window(&IpcDead, "foot-t1", deadline).unwrap_err();
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "IPC 錯誤不得進入 200ms 輪詢"
        );
        let msg = format!("{err:#}");
        assert!(msg.contains("IPC 不可用"), "須歸因 IPC 層：{msg}");
        assert!(
            msg.contains("os error 111") || msg.contains("Connection refused"),
            "須保留根因：{msg}"
        );
        assert!(
            !msg.contains("WaitWindow 逾時"),
            "IPC 錯誤不得偽裝成視窗未出現：{msg}"
        );
    }

    #[test]
    fn wait_window_not_found_retries_until_success() {
        let stub = FlakyFound(std::sync::atomic::AtomicUsize::new(0));
        let deadline = Instant::now() + Duration::from_secs(5);
        let geo = wait_for_window(&stub, "foot-t1", deadline).unwrap();
        assert_eq!((geo.x, geo.y, geo.width, geo.height), (1, 2, 10, 20));
        assert_eq!(stub.0.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[test]
    fn wait_window_timeout_message_keeps_structure_and_cause() {
        // deadline 已過 → 一次查詢即回報（不得長眠），訊息含最後查無視窗根因
        let deadline = Instant::now();
        let started = Instant::now();
        let err = wait_for_window(&NeverFound, "foot-t1", deadline).unwrap_err();
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "deadline 已過不得再重試"
        );
        let msg = format!("{err:#}");
        assert!(msg.contains("WaitWindow 逾時"), "{msg}");
        assert!(msg.contains("foot-t1"), "{msg}");
        assert!(
            msg.contains("找不到符合"),
            "逾時訊息須含最後一次查無視窗原因：{msg}"
        );
    }

    #[test]
    fn is_ipc_error_distinguishes_layers() {
        let ipc: anyhow::Error = CompositorError::Ipc("x".to_owned()).into();
        assert!(is_ipc_error(&ipc));

        let nf: anyhow::Error = CompositorError::NotFound("y".to_owned()).into();
        assert!(!is_ipc_error(&nf), "查無視窗不屬 IPC 層");

        let plain: anyhow::Error = anyhow!("其他錯誤");
        assert!(!is_ipc_error(&plain));
    }

    // ── finalize_recorder：優雅收尾訊號（SIGINT → 逾時升級 SIGKILL）──

    // 模擬 child：trap 安裝後無限迴圈的 sh 處理程序（不要求真 wf-recorder）
    #[cfg(unix)]
    #[tokio::test]
    async fn finalize_recorder_graceful_sigint_exits_zero() {
        let mut child = TokioCommand::new("sh")
            .arg("-c")
            .arg("trap 'exit 0' INT; while :; do sleep 0.05; done")
            .spawn()
            .unwrap();
        // 給 sh 時間安裝 trap（真實 recorder 上線後才會收尾，無此競態）
        tokio::time::sleep(Duration::from_millis(200)).await;
        let status = finalize_recorder(&mut child, RECORDER_SHUTDOWN_GRACE)
            .await
            .unwrap();
        // 優雅訊號 → trap 觸發 exit 0（對應 wf-recorder 正常 finalize 完檔）
        assert!(status.success(), "SIGINT 優雅收尾應 exit 0：{status:?}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn finalize_recorder_escalates_to_sigkill_on_timeout() {
        // trap '' = 忽略 SIGINT/SIGTERM，只能靠逾時後的 SIGKILL 終結
        let mut child = TokioCommand::new("sh")
            .arg("-c")
            .arg("trap '' INT TERM; while :; do sleep 0.05; done")
            .spawn()
            .unwrap();
        // 給 sh 時間安裝 trap
        tokio::time::sleep(Duration::from_millis(200)).await;
        let status = finalize_recorder(&mut child, Duration::from_millis(300))
            .await
            .unwrap();
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(status.code(), None, "應被訊號終結而非正常退出");
        assert_eq!(status.signal(), Some(libc::SIGKILL), "逾時應升級 SIGKILL");
    }
}
