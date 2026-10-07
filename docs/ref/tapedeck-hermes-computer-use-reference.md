# Hermes Computer Use Reference

> Reference notes for TapeDeck  
> Source: Hermes Agent official documentation / NousResearch  
> Retrieved: 2026-10-08  
> Purpose: preserve the Hermes Computer Use architecture and action model as a reference for future TapeDeck / `.roll` research.
>
> This document is a **reference summary**, not a reproduction of the upstream documentation.

## 1. Overview

Hermes Agent provides a `computer_use` toolset for driving a desktop in the background on:

- macOS
- Windows
- Linux

The design emphasizes co-working: the agent can interact with applications without taking over the user's real cursor, keyboard focus, or virtual desktop.

Hermes is also **model-neutral**. The Computer Use interface can be used with tool-capable models such as Claude, GPT, Gemini, or open models exposed through an OpenAI-compatible endpoint.

The built-in Hermes `computer_use` toolset communicates with `cua-driver` through MCP over stdio.

## 2. Architecture

```text
Hermes Agent
    |
    | computer_use toolset
    | MCP / stdio
    v
cua-driver
    |
    +-- Accessibility tree
    |
    +-- Synthetic input dispatch
    |
    +-- Screenshot / window state
    |
    +-- Cross-platform desktop control
    |
    v
Operating System
```

The important architectural separation is:

```text
Hermes
  = agent-facing Computer Use interface

cua-driver
  = low-level cross-platform desktop driver

OS accessibility + input APIs
  = platform implementation
```

Hermes deliberately exposes a higher-level action vocabulary instead of requiring the agent to speak the driver's raw MCP vocabulary.

## 3. Platform implementation

| Platform | Accessibility | Input dispatch |
|---|---|---|
| macOS | AX / SkyLight mechanisms | `SLPSPostEventRecordTo` |
| Windows | UIAutomation | `SendInput` + `PostMessage` |
| Linux X11 | AT-SPI | XTest |
| Linux Wayland | AT-SPI | virtual-keyboard |

The common goal is to let the agent inspect the accessibility tree and synthesize input without unnecessarily bringing a window to the foreground, stealing focus, switching virtual desktops, or moving the user's real cursor.

## 4. Background / no-foreground model

A key Hermes/Cua design principle is the **no-foreground contract**.

Conceptually:

```text
User is working normally
        |
        +--------------------+
        |                    |
        v                    v
real cursor / keyboard    Agent overlay / driver
        |                    |
        +---------+----------+
                  |
                  v
             same desktop
```

The agent's actions are separate from the user's physical pointer.

This is particularly important for a future recording system because the event stream can represent agent actions independently of the user's physical input device.

## 5. Hermes action layer

Hermes exposes a higher-level `computer_use` action vocabulary.

The documented workflow begins with capture:

```text
computer_use(
  action="capture",
  mode="som",
  app="..."
)
```

A capture can provide:

1. screenshot
2. indexed accessibility / semantic elements

Example:

```text
#1  AXButton 'Back' @ (...)
#2  AXTextField 'Address bar' @ (...)
#7  Link 'Sign In' @ (...)
```

The agent can then act on an element:

```text
computer_use(action="click", element=7)
```

or use actions such as:

```text
click
type
key
scroll
drag
mouse_move
wait
capture
```

The exact action vocabulary is intentionally a Hermes-facing abstraction and should not be confused with the lower-level `cua-driver` MCP vocabulary.

## 6. Semantic element handles

One particularly useful design is the element-index mechanism.

A capture creates a snapshot containing indexed elements:

```text
snapshot
  |
  +-- #1 Button
  +-- #2 TextField
  +-- #3 Link
  +-- ...
```

The agent refers to the element by its index.

Conceptually:

```text
capture
   ↓
snapshot_id + element list
   ↓
agent selects element #N
   ↓
Hermes resolves #N
   ↓
cua-driver element token
   ↓
input dispatch
```

Element indexes are intentionally short-lived. After a state-changing operation, the agent should capture again rather than relying on stale indexes.

This is an important safety property: stale references should fail rather than silently target a different UI element.

## 7. Accessibility-first interaction

Hermes encourages accessibility / semantic interaction where possible.

The accessibility tree can provide:

- role
- label / name
- bounds
- application
- element identity

This can reduce dependence on raw pixel coordinates.

A useful conceptual priority is:

```text
semantic element
      ↓
accessibility tree
      ↓
coordinate fallback
      ↓
raw pointer event
```

This is relevant to TapeDeck because future recordings may benefit from retaining both:

- semantic target information
- physical input information

rather than recording coordinates alone.

## 8. Browser vs desktop boundary

Hermes distinguishes browser-page automation from desktop Computer Use.

For web-page content, browser-specific tools should generally be preferred when they can directly access the DOM/page structure.

Computer Use is particularly useful for:

- native applications
- browser chrome
- dialogs
- OS-level UI
- authentication dialogs
- file pickers
- applications without a suitable browser automation interface

Conceptually:

```text
Web page
  -> browser / DOM tools

Native / OS UI
  -> computer_use
```

## 9. Driver-side vocabulary

The underlying `cua-driver` exposes a lower-level MCP interface.

Hermes maps its own actions to driver concepts such as:

```text
Hermes
  capture
      -> driver get_window_state

Hermes
  element=N
      -> driver element_token
```

The driver also uses state/snapshot concepts such as:

```text
snapshot_id
element_token
```

TapeDeck should treat this as implementation detail rather than assuming the Hermes wrapper is the universal Computer Use IR.

## 10. Cross-platform design

The same high-level Hermes Computer Use concept maps to platform-specific primitives:

```text
                Hermes computer_use
                        |
          +-------------+-------------+
          |             |             |
        macOS         Windows       Linux
          |             |             |
         AX            UIA          AT-SPI
          |             |             |
       SkyLight      SendInput      XTest /
       dispatch      PostMessage    Wayland VK
```

This is a useful model for TapeDeck:

> Keep the recorded semantic action independent from the OS-specific dispatch mechanism whenever possible.

## 11. Windows relevance

Hermes supports Windows through UIAutomation and synthesized input.

Windows-specific details include:

- UIAutomation accessibility tree
- `SendInput`
- `PostMessage`
- optional driver autostart
- Session 0 / interactive-session considerations when driving Windows remotely over SSH

This makes Hermes particularly useful as a reference implementation for exploring Windows Computer Use without TapeDeck having to implement the Windows hardware/input layer first.

## 12. Linux / Wayland relevance

Linux supports:

- X11 via XTest
- Wayland via virtual-keyboard
- AT-SPI for accessibility

Native Wayland support is an explicit opt-in in current Hermes configuration.

For the user's environment, this is especially relevant because Auraspark/LoomCowork already targets Wayland-based systems.

## 13. Permissions and safety

Hermes has platform-specific permission and capability handling.

Current configuration includes concepts such as:

```yaml
computer_use:
  permission_mode: standard
  capability_manifest: ""
```

A bounded mode can use a capability manifest to restrict allowed applications, browser profiles, origins, and typed tools. Requests outside the manifest can fail closed.

This suggests a useful TapeDeck principle:

> Recording should preserve not only what happened, but potentially the capability / authorization context under which the operation was allowed.

Do not store secrets merely because they were present in the UI.

## 14. Telemetry

Hermes disables `cua-driver` anonymous telemetry by default when invoking the driver.

A configuration option can opt back into driver telemetry.

For TapeDeck research, telemetry should remain conceptually separate from operation recording.

## 15. Recording relevance

The Hermes architecture is especially interesting for TapeDeck because it naturally exposes several layers:

```text
Intent
  |
  v
Hermes action
  |
  v
Semantic target
  |
  v
Driver operation
  |
  v
OS-specific event
  |
  v
Observed state
```

A future TapeDeck recording could potentially preserve multiple layers:

```text
timestamp
session
application
window
observation
semantic_target
action
arguments
driver_event
result
```

However, this document does **not** define a TapeDeck recording format.

The important research question is:

> Which information remains stable across Hermes, OpenAI Computer Use, Anthropic Computer Use, cua-driver, and real execution traces?

That stable intersection should be discovered empirically before defining a universal Execution IR.

## 16. Recommended future TapeDeck investigation

Do not immediately wrap Hermes' schema into a TapeDeck standard.

Instead:

1. Run Hermes Computer Use on Linux/Wayland.
2. Run it on Windows.
3. Capture real traces.
4. Inspect accessibility snapshots.
5. Record action → observation transitions.
6. Compare semantic element references with coordinates.
7. Compare Hermes actions with raw cua-driver operations.
8. Compare those traces with OpenAI and Anthropic Computer Use.
9. Identify the smallest stable semantic vocabulary.
10. Only then consider a TapeDeck recording / execution IR.

This follows the broader TapeDeck philosophy:

```text
real traces
   ↓
observed common semantics
   ↓
canonical IR
```

rather than:

```text
provider schema
   ↓
premature universal DSL
```

## 17. Relationship to `.roll`

Hermes Computer Use is a **reference input/execution system** for TapeDeck research.

It is not the TapeDeck `.roll` format.

Keep the boundaries explicit:

```text
Hermes
  = agent / worker

computer_use
  = agent-to-environment interface

cua-driver
  = desktop driver

TapeDeck
  = operation recording / analysis system

.roll
  = future TapeDeck recording format
```

TapeDeck may eventually record Hermes operations, but Hermes should not become a dependency of the `.roll` format.

## 18. Primary references

Official Hermes Computer Use documentation:
- https://hermes-agent.nousresearch.com/docs/user-guide/features/computer-use

Official Hermes Computer Use skill:
- https://github.com/NousResearch/hermes-agent/blob/main/skills/autonomous-ai-agents/computer-use/SKILL.md

Official Hermes tools reference:
- https://github.com/NousResearch/hermes-agent/blob/main/website/docs/reference/tools-reference.md

The Hermes documentation also points to the `cua-driver` project for lower-level platform, recording, browser, and driver behavior.

## 19. Key takeaways

### For Hermes

```text
model-neutral
+ MCP
+ semantic accessibility tree
+ background desktop control
+ cross-platform driver
+ higher-level action wrapper
```

### For TapeDeck

The most valuable reference points are:

1. semantic accessibility snapshots
2. short-lived element handles
3. action → observation loops
4. separation of agent action from OS dispatch
5. background/no-foreground execution
6. cross-platform driver abstraction
7. explicit permission/capability boundaries
8. real execution traces before defining a universal IR

### Final principle

> **TapeDeck should learn from Hermes' traces, not become a copy of Hermes' schema.**

The goal is to discover the durable semantics of computer operation first, then decide what `.roll` should preserve.
