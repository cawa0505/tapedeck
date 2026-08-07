# tapedeck Scripting DSL Concept

## Purpose
Human‑readable, declarative scripting language for tapedeck that can describe:
- **What** to record (window, screen, region)
- **How long** to record
- **Which engine** to use (wf-recorder, ffmpeg, etc.)
- **Pre‑/post‑execution hooks**
- **Post‑processing** (format conversion, optimisation)
- **Fancy actions** (mouse clicks, typing)

The format enables AI agents to generate complete, reproducible recordings with a few lines of config, while keeping the underlying bash/ffmpeg complexity hidden inside tapedeck.

---

## Top‑Level Structure

```yaml
# Example: zola_demo.rec
target:
  type:   window          # | screen | region
  id:     "Zola Development Server"
engine:   wf-recorder     # wf-recorder | ffmpeg | gstreamer
fps:      60
output:   "assets/zola_preview.mp4"
duration: "5s"            # support units: s, ms, h, d

hooks:
  before: ["systemctl start zola"]
  after:  ["pkill zola"]

post:
  format:   mp4           # target format: mp4 | gif | webm
  preset:   "hd"          # optional: "low", "hd", "ultra"
  optimize: true
```

### Detailed Field Reference

| Field | Type | Description |
|-------|------|-------------|
| `target.type` | enum | `window` – capture a specific X11/Wayland window by title/app_id. `screen` – capture the entire screen (first monitor). |
| `target.id` | string | Identifier for the target (window title, app_id, or for screen it can be omitted). |
| `engine` | enum | `wf-recorder` (native Wayland), `ffmpeg` (fallback X11/Wayland), `gstreamer` (future). |
| `fps` | u32 | Frames per second of the recording. |
| `output` | string | Destination path (e.g., `assets/demo.gif`). Extension dictates default format if omitted. |
| `duration` | string | Recording length (`10s`, `500ms`, `2h`). Parsed with `humantime` crate. |
| `hooks.before` | array[str] | Commands executed **before** the recorder starts. |
| `hooks.after` | array[str] | Commands executed **after** the recorder stops. |
| `post.format` | enum | `mp4`, `gif`, `webm`. |
| `post.preset` | enum | `low` / `hd` / `ultra` – controls encoder preset (e.g., `-crf 23`). |
| `post.optimize` | boolean | Run post‑processing optimisation (palettegen/paletteuse for GIF, `-movflags +faststart` for MP4). |
| `post.filters` | array[str] *(optional)* | Additional ffmpeg filters (e.g., `uq=1`, `scale=1920:1080`). |

---

## Execution Flow (Rust pseudo‑logic)

```rust
pub struct RecorderScript {
    target: Target,
    engine: RecordingEngine,
    fps: u32,
    output: String,
    duration: Duration,
    hooks: Hooks,
    post: PostProcess,
}

impl RecorderScript {
    pub async fn execute(&self) -> anyhow::Result<()> {
        // 1️⃣ Hook → before
        for cmd in &self.hooks.before {
            Command::new("sh").arg("-c").arg(cmd).spawn()?;
        }

        // 2️⃣ Launch recording engine
        let mut recorder = match self.engine {
            RecordingEngine::WfRecorder => spawn_wf_recorder(self)?,
            RecordingEngine::FFmpeg => spawn_ffmpeg(self)?,
        };

        // 3️⃣ Wait the requested duration
        tokio::time::sleep(self.duration).await;

        // 4️⃣ Graceful stop
        recorder.kill()?;

        // 5️⃣ Post‑processing (format, optimisation)
        if self.post.optimize {
            self.apply_post_processing().await?;
        }

        // 6️⃣ Hook → after
        for cmd in &self.hooks.after {
            Command::new("sh").arg("-c").arg(cmd).spawn()?;
        }

        Ok(())
    }
}
```

---

## Integration with Existing tapedeck Features

1. **CLI Mode**  
   - `tapedeck run --script path/to/file.rec`  
   - `tapedeck run --path path/to/file.rec` (same, just syntactic sugar)

2. **TUI Mode**  
   - `tapedeck` displays two panes:
     - **Left** – `fzf` list of `.tape` *and* `.rec` files.  
     - **Right** – Preview pane showing the script contents.  
   - Press **r** → execut **run script in background** and close the UI when done.  
   - Press **p** → preview generated media using `kitty`/`sixel` if available, otherwise fallback to `chafa` ASCII/Unicode block.

3. **MCP Mode**  
   - AI agents can invoke the single tool `record_script`:
     ```json
     {
       "name": "record_script",
       "description": "Execute a declarative recording script (.rec / .yml / .yaml).",
       "inputSchema": {
         "type": "object",
         "properties": {
           "script_path": { "type": "string" }
         },
         "required": ["script_path"]
       }
     }
     ```
   - The MCP server resolves the script, parses it, and streams back operation status.

---

## Example Scripts

### 1. Simple Wayland Capture

```yaml
# capture_zola.rec
target:
  type:   window
  id:     "Zola Development Server"
engine:   wf-recorder
fps:      60
output:   "assets/zola.mp4"
duration: "8s"

hooks:
  before: ["zola serve"]
  after:  ["pkill zola"]
```

### 2. Multi‑Window Demo with GIF Optimisation

```yaml
# demo_multi.rec
target:
  type:   region
  id:     "0x0/640x480+100+200"   # x,y,width,height (optional)
engine:   ffmpeg
fps:      30
output:   "assets/demo.gif"
duration: "10s"
post:
  format:   gif
  preset:   "hd"
  optimize: true
```

### 3. With Mouse Click Simulation

```yaml
# click_demo.rec
target:
  type:   window
  id:     "my-app"
engine:   wf-recorder
fps:      60
output:   "assets/click.gif"
duration: "5s"
post:
  format:   gif
  preset:   "hd"
exec:
  - "sleep 1"
  - "xte 'mouse 500 300'"
```

---

## Implementation Notes

| Task | File | Priority |
|------|------|----------|
| Add `serde_yaml` dependency | `Cargo.toml` | high |
| Parse `.rec` / `.yml` → `RecorderScript` | `engine/script/parser.rs` | high |
| Dispatch recording engine based on `engine` field | `engine/` modules | high |
| Extend MCP tool `record_tui_tape` to support script input | `mcp/tools.rs` | medium |
| TUI preview of script contents | `tui/ui.rs` | medium |
| CLI `--script` sub‑command | `cli.rs` | low |
| CI workflow for script validation | `.github/workflows/ci.yml` | low |

---

## Risks & Mitigations

| Risk | Description | Mitigation |
|------|-------------|------------|
| **YAML parsing errors** | Malformed scripts could crash the recorder. | Validate parsed `RecorderScript` before execution; use `serde_yaml::from_str` with `Result` and return human‑readable error messages. |
| **Engine binary not installed** | Users may lack `wf-recorder` or `ffmpeg`. | Provide helpful error at startup (`cargo run --features "wf-recorder"`) and a `setup` hook that suggests `apt get wf-recorder` etc. |
| **Hook command failures** | `before` / `after` commands may block or exit non‑zero. | Run them in a temporary background process; ignore failures unless the wrapper flags `--strict`. |

---

## How to Store This Document

```
# In the repository
/docs/
    DSL_Concept.md      ← (this file)
src/
    engine/
        script/
            parser.rs
            ast.rs
```

Add `serde_yaml = "0.9"` to `Cargo.toml` under `[dependencies]`.

---

*Prepared for the tapedeck project – enables AI‑native, script‑driven recordings without exposing low‑level command‑line juggling.*