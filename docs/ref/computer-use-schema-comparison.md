# Computer Use Schema Comparison
## OpenAI / Codex vs Anthropic Claude

- **Status:** Research / Architecture Reference
- **Date:** 2026-10-08
- **Purpose:** 整理目前公開的 Computer Use action schema，作為未來 Auraspark / Execution IR 設計的參考
- **Scope:** OpenAI Computer Use（Codex 使用的 Computer Use 能力所屬的公開 API schema）與 Anthropic Computer Use
- **Important:** 本文件不是把任何一家 schema 當成新的 Auraspark DSL；目的是觀察兩家實際採用的共同 primitive。

---

# 1. Executive Summary

目前 OpenAI 與 Anthropic 都已經公開了相當成熟的 Computer Use interface。

兩者的共同結構非常明顯：

```text
Agent
  ↓
structured computer action
  ↓
host / adapter
  ↓
actual desktop / browser
  ↓
screenshot / observation
  ↓
Agent
```

但是兩家的 schema 並不相同。

## OpenAI

OpenAI 的 Computer Use tool 以：

```text
computer_call
  └── actions[]
```

為核心。

目前公開文件列出的 action primitive 包括：

```text
click
double_click
drag
move
scroll
keypress
type
wait
screenshot
```

模型回傳 ordered actions，由 host application 依序執行，再回傳 screenshot / observation。

OpenAI 同時提供另一條路線：讓模型產生 code，透過 Playwright、PyAutoGUI 等 library 操作環境。官方目前對較新的模型推薦 code execution；structured computer tool 仍是支援中的方案。

## Anthropic

Anthropic 最新的 Computer Use 是：

```text
computer_toolset_20260801
```

它不是讓使用者自行定義 JSON schema，而是 Anthropic 內建 schema 的 client toolset。

目前公開文件列出 17 個 member tools：

```text
screenshot
left_click
right_click
middle_click
double_click
triple_click
left_click_drag
mouse_move
left_mouse_down
left_mouse_up
cursor_position
scroll
type
key
hold_key
wait
zoom
```

Claude 以 `tool_use` block 呼叫 member tool：

```json
{
  "type": "tool_use",
  "name": "left_click",
  "toolset_name": "computer",
  "input": {
    "coordinate": [640, 60]
  }
}
```

Host 執行後，以 `tool_result` 回傳結果。

---

# 2. OpenAI Computer Use

Official documentation:

- OpenAI Computer Use guide
- OpenAI API reference for computer-use actions

OpenAI 的 Computer Use 是一個 client-side execution model：

```text
OpenAI model
    ↓
computer_call
    ↓
your application
    ↓
browser / desktop
    ↓
screenshot
    ↓
OpenAI model
```

官方文件明確指出，application 必須負責執行 model 回傳的 action。

---

# 3. OpenAI Action Schema

概念上：

```json
{
  "type": "computer_call",
  "call_id": "call_002",
  "actions": [
    {
      "type": "click",
      "button": "left",
      "x": 405,
      "y": 157
    },
    {
      "type": "type",
      "text": "penguin"
    }
  ],
  "status": "completed"
}
```

Action 是 ordered list。

目前公開文件列出的 action 類型：

```text
click
double_click
drag
move
scroll
keypress
type
wait
screenshot
```

## 3.1 click

概念：

```json
{
  "type": "click",
  "button": "left",
  "x": 405,
  "y": 157
}
```

OpenAI 的公開 API reference 目前也將 click button 擴展到：

```text
left
right
wheel
back
forward
```

並可帶：

```text
keys[]
```

表示操作時保持按住的按鍵。

例如概念上：

```json
{
  "type": "click",
  "button": "left",
  "x": 500,
  "y": 300,
  "keys": ["SHIFT"]
}
```

## 3.2 double_click

```json
{
  "type": "double_click",
  "x": 500,
  "y": 300
}
```

## 3.3 drag

概念上：

```json
{
  "type": "drag",
  "path": [
    {"x": 200, "y": 300},
    {"x": 400, "y": 300},
    {"x": 600, "y": 350}
  ]
}
```

重點不是具體 path serialization，而是 OpenAI 將 drag 視為一個 structured computer action，而不是要求 agent 自己產生 OS-specific mouse API。

## 3.4 move

```json
{
  "type": "move",
  "x": 500,
  "y": 300
}
```

用於移動游標 / hover。

## 3.5 scroll

概念：

```json
{
  "type": "scroll",
  "x": 500,
  "y": 400,
  "scroll_x": 0,
  "scroll_y": 3
}
```

實際 API schema 以目前 OpenAI API reference 為準。

## 3.6 keypress

概念：

```json
{
  "type": "keypress",
  "keys": [
    "CTRL",
    "S"
  ]
}
```

## 3.7 type

```json
{
  "type": "type",
  "text": "hello world"
}
```

## 3.8 wait

```json
{
  "type": "wait"
}
```

## 3.9 screenshot

```json
{
  "type": "screenshot"
}
```

第一個 computer call 可以只包含 screenshot。

這是一個很重要的設計：

```text
observe
  ↓
act
  ↓
observe
```

而不是一次產生整個 deterministic script。

---

# 4. OpenAI 的另一條路：Code Execution

OpenAI 現在同時支持：

```text
Computer Use
    = structured UI actions

Code Execution
    = model writes code
      ↓
      Playwright / PyAutoGUI / other libraries
```

官方目前對較新的模型推薦 code execution，而 Computer Use 是另一個支援方案。

這表示 OpenAI 並沒有把 Computer Use action schema 定位成「所有 agent execution 的唯一語言」。

這點對 Auraspark 很重要。

---

# 5. Codex 與 OpenAI Computer Use

Codex 本身是一個 agent/product，而不是一份獨立、公開的「Codex Computer Use DSL specification」。

因此比較準確的說法是：

```text
Codex
  ↓
OpenAI agent / computer-use capabilities
  ↓
Computer Use action interface
```

Codex 在 Windows 上也已經支援 Computer Use；OpenAI 公開說明中描述它可以讓 Codex 在 Windows application 中：

```text
see
click
type
```

因此，如果我們想研究：

> 「Codex 會怎麼操作 Windows？」

目前比較合理的研究對象不是找一份叫「Codex DSL」的規格，而是研究 OpenAI 的 Computer Use interface，以及 Codex 實際產生的行為。

---

# 6. Anthropic Computer Use

Anthropic 最新公開版本：

```text
computer_toolset_20260801
```

它是一個 Anthropic-defined client toolset。

工具定義：

```json
{
  "type": "computer_toolset_20260801"
}
```

不需要自行提供 input schema。

Anthropic 已經把 schema 內建在模型 / toolset 中。

---

# 7. Anthropic 17 Member Tools

## Observation

```text
screenshot
zoom
cursor_position
```

## Mouse

```text
left_click
right_click
middle_click
double_click
triple_click
left_click_drag
mouse_move
left_mouse_down
left_mouse_up
```

## Keyboard

```text
type
key
hold_key
```

## Timing

```text
wait
```

完整列表：

```text
01 screenshot
02 left_click
03 right_click
04 middle_click
05 double_click
06 triple_click
07 left_click_drag
08 mouse_move
09 left_mouse_down
10 left_mouse_up
11 cursor_position
12 scroll
13 type
14 key
15 hold_key
16 wait
17 zoom
```

---

# 8. Anthropic Action Examples

## screenshot

```json
{
  "type": "tool_use",
  "name": "screenshot",
  "toolset_name": "computer",
  "input": {}
}
```

## left_click

```json
{
  "type": "tool_use",
  "name": "left_click",
  "toolset_name": "computer",
  "input": {
    "coordinate": [640, 60]
  }
}
```

## type

```json
{
  "type": "tool_use",
  "name": "type",
  "toolset_name": "computer",
  "input": {
    "text": "pictures of cats"
  }
}
```

## key

```json
{
  "type": "tool_use",
  "name": "key",
  "toolset_name": "computer",
  "input": {
    "text": "ctrl+s",
    "repeat": 1
  }
}
```

## left_click_drag

```json
{
  "type": "tool_use",
  "name": "left_click_drag",
  "toolset_name": "computer",
  "input": {
    "start_coordinate": [200, 300],
    "coordinate": [600, 300]
  }
}
```

## scroll

```json
{
  "type": "tool_use",
  "name": "scroll",
  "toolset_name": "computer",
  "input": {
    "coordinate": [500, 400],
    "scroll_direction": "down",
    "scroll_amount": 3
  }
}
```

## zoom

```json
{
  "type": "tool_use",
  "name": "zoom",
  "toolset_name": "computer",
  "input": {
    "region": [100, 200, 400, 350]
  }
}
```

---

# 9. Anthropic 的重要設計差異

Anthropic 把 action schema 拆得比 OpenAI 細。

例如：

```text
OpenAI
  click
  drag
  move
  keypress

Anthropic
  left_click
  right_click
  middle_click
  double_click
  triple_click
  left_click_drag
  mouse_move
  left_mouse_down
  left_mouse_up
  key
  hold_key
```

也就是 Anthropic 更接近：

> **低階 computer primitive**

而 OpenAI 的 schema 比較接近：

> **較高階的 computer action**

兩者都不是完整的 workflow DSL。

---

# 10. Side-by-Side

| Semantic | OpenAI | Anthropic |
|---|---|---|
| Screenshot | `screenshot` | `screenshot` |
| Click | `click` | `left_click` |
| Right click | `click(button=right)` | `right_click` |
| Double click | `double_click` | `double_click` |
| Triple click | — | `triple_click` |
| Mouse move | `move` | `mouse_move` |
| Drag | `drag` | `left_click_drag` |
| Raw mouse down | — | `left_mouse_down` |
| Raw mouse up | — | `left_mouse_up` |
| Cursor position | — | `cursor_position` |
| Scroll | `scroll` | `scroll` |
| Type | `type` | `type` |
| Key | `keypress` | `key` |
| Hold key | — | `hold_key` |
| Wait | `wait` | `wait` |
| Region observation | — | `zoom` |

這個表其實已經非常接近一個跨 provider semantic mapping。

---

# 11. 找共同 Primitive

如果故意不看 provider-specific naming，可以抽象出：

```text
Observation
├── screenshot
└── inspect_region

Pointer
├── move
├── click
├── double_click
├── drag
├── button_down
└── button_up

Keyboard
├── type
├── key
└── hold

Navigation
└── scroll

Synchronization
└── wait
```

這就是目前兩家公開 schema 最有價值的共同部分。

---

# 12. 更重要的共同結構：Action + Observation Loop

兩家真正共同的不是 action 名稱，而是 loop：

```text
┌──────────────┐
│    Agent     │
└──────┬───────┘
       │
       │ action
       ▼
┌──────────────┐
│ Host Adapter │
└──────┬───────┘
       │
       │ execute
       ▼
┌──────────────┐
│ Environment  │
└──────┬───────┘
       │
       │ observation
       ▼
┌──────────────┐
│    Agent     │
└──────────────┘
```

而不是：

```text
Agent
  ↓
generate entire script
  ↓
execute
```

這對你未來的 Execution IR 設計非常重要。

---

# 13. 對 Auraspark 的意義

我現在不建議 Auraspark 直接採用：

```text
OpenAI schema
```

也不建議：

```text
Anthropic schema
```

更不建議：

```text
把兩個 schema 拼在一起
```

比較好的做法是把它們當作兩個 adapter：

```text
                 Execution IR
                      │
          ┌───────────┴───────────┐
          ▼                       ▼
 OpenAI Computer Use       Anthropic Computer Use
          │                       │
          ▼                       ▼
       Adapter                 Adapter
          │                       │
          └───────────┬───────────┘
                      ▼
                Actual Runtime
```

但是——

**現在連 Execution IR 都不需要急著做。**

---

# 14. 更值得保存的是 Semantic Mapping

未來如果真的需要，可以建立：

```yaml
semantic:
  pointer.click:
    openai: click
    anthropic: left_click

  pointer.double_click:
    openai: double_click
    anthropic: double_click

  pointer.move:
    openai: move
    anthropic: mouse_move

  pointer.drag:
    openai: drag
    anthropic: left_click_drag

  keyboard.type:
    openai: type
    anthropic: type

  keyboard.key:
    openai: keypress
    anthropic: key

  observation.screenshot:
    openai: screenshot
    anthropic: screenshot

  synchronization.wait:
    openai: wait
    anthropic: wait
```

這比較像：

> **Provider Capability Mapping**

而不是新的 DSL。

---

# 15. 目前還不能抽象成共同標準的部分

以下目前不要急著標準化：

```text
coordinate semantics
screen scaling
zoom behavior
drag path representation
keyboard naming
modifier representation
scroll units
screenshot resolution
tool result format
approval / permission model
session model
browser vs desktop distinction
```

尤其 coordinate。

Anthropic 明確規定 coordinate 是以返回的 full-display screenshot pixel space 為基準；OpenAI 也有自己的 coordinate / screenshot handling 規則。

因此：

```text
(x, y)
```

雖然看起來相同，並不代表兩家 runtime 的 coordinate semantics 已經是一個標準。

---

# 16. Browser 也是另一個問題

Computer Use 不等於 Browser Automation。

現在兩家都逐漸把這兩件事情分開：

```text
Browser automation
  = DOM / page semantics

Computer use
  = pixels / mouse / keyboard
```

例如 Anthropic 明確建議，如果工作只停留在 webpage 裡，應使用 Browser Use tool，而不是完整 Computer Use。

OpenAI 也同時提供：

```text
Playwright / code execution
computer actions
```

因此未來 Auraspark 如果做 Execution IR，可能會需要：

```text
Execution
├── semantic browser operations
│     ├── navigate
│     ├── click_element
│     ├── fill
│     └── extract
│
└── computer operations
      ├── click
      ├── type
      ├── key
      └── screenshot
```

這兩層不要混成一個 coordinate DSL。

---

# 17. 對你目前架構最重要的結論

我現在會把我們之前的想法再往前修正一步：

```text
Courier
  = Work Contract

Computer Use
  = Agent → Environment action interface

Execution IR
  = 未來可能出現的 provider-neutral semantic layer

.roll / TapeDeck
  = 完全另一條產品線
```

所以：

```text
Courier
    │
    ▼
Agent
    │
    ├── OpenAI Computer Use
    ├── Anthropic Computer Use
    ├── Playwright
    ├── MCP tools
    ├── shell
    └── other capabilities
```

**Courier 不需要知道其中任何一個的 action schema。**

---

# 18. 最有趣的觀察

其實目前兩家已經共同證明：

> **Agent computer control 的最低共同語言，正在收斂到「Observation + Primitive Action + Observation」。**

而 primitive 大致就是：

```text
observe
click
move
drag
scroll
type
key
wait
```

所以如果未來真的要設計 Auraspark Execution IR，我會從這個 intersection 開始，而不是從 OpenAI 或 Anthropic 任一家 schema 開始。

更進一步：

```text
Courier
   ↓
"What"

Agent
   ↓
"How"

Provider Computer Use
   ↓
"Primitive action"

Runtime
   ↓
"Do"

Observation
   ↓
"Did it work?"
```

這樣就能非常自然地維持我們剛才確立的產品邊界。

---

# 19. Sources

OpenAI Computer Use：
https://developers.openai.com/api/docs/guides/tools-computer-use

OpenAI API computer-use action reference：
https://developers.openai.com/api/reference/

Anthropic Computer Use：
https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool

Anthropic Tool Reference：
https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-reference

本文件中的 schema 名稱、action 列表與介面描述以 2026-10-08 公開文件為準；provider schema 可能隨模型與 tool version 演進。
