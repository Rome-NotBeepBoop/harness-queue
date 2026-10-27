# Harness Queue (`hq`)

Minimal, universal, terminal-based agent queue that works alongside existing coding-agent harnesses inside **Orca IDE**, **Herdr**, and **Ghostty** terminal panes.

```text
┌──────────────────────────────────────────────────────────┐
│ Harness Queue (hq) v0.1                                  │
│ Host: Orca IDE · Location: term_d5d8770b                 │
│ Target: OpenAI Codex (term_7b2d3b68)                     │
└──────────────────────────────────────────────────────────┘

? Select mode:
  1) Auto-resume  (default — press [Enter])
     Resume with the built-in continuation prompt.

  2) Auto-send
     Queue your own prompts for automatic delivery.

› Choice [1] or enter prompt directly: 
```

---

## What's New

- **Interactive TUI Mode (`hq --tui`)**: Fullscreen terminal interface powered by `ratatui` with live split-panel overview of queued items, real-time agent status badges (`● WATCHING`, `⚙️ BUSY`, `⏳ QUOTA WAIT`, `✓ READY`), scrollable event logs, and an interactive prompt input field.
- **Unified CLI (`hq` or `hq --cli`)**: Calling `hq` or `hq --cli` runs the default CLI experience. Only `--tui` differs.
- **Infinite / Multiple Prompt Queueing**: Queue prompt `#1`, `#2`, `#3`... indefinitely! While the agent is working or waiting for quota resets, you can queue subsequent prompts anytime. They are dispatched automatically in FIFO order as soon as the agent becomes ready.
- **Image Compatibility**: Seamlessly paste image paths (e.g. `@/var/folders/.../orca-paste-....png`, direct file paths, or relative paths). Images are automatically detected, validated for file existence, sized, and forwarded with rich multimodal context to agents like Codex, Claude Code, and Pi.

---

## Core Philosophy & Design

1. **Terminal Auto-Detection**: Works seamlessly inside **Orca IDE** terminal splits, **Herdr** panes, and **Ghostty** split terminals with zero manual configuration.
2. **Pure Open-Source CLI & IPC**: **Zero AppleScript**, zero accessibility hacks, zero synthetic mouse movements. Built purely with native CLI and JSON-RPC APIs for full open-source portability.
3. **Zero Wrappers Required**: Launch your agent normally (OpenAI Codex, Claude Code, OpenCode, OMP, Pi, Google Antigravity). HQ automatically discovers and pairs with the neighboring agent in the same workspace folder.
4. **Intuitive 0-Learning-Curve UX**:
   - Press **`[Enter]`** to start Auto-resume immediately.
   - Or **type your prompt directly** to queue Prompt #1 without navigating extra menus.
   - Or run **`hq "your prompt"`** from the command line to start queueing in one step.
5. **Two Mutually Exclusive Modes**:
   - **Mode 1 (Auto-resume)**: Predefined prompt (`"Continue the unfinished work. Inspect the session history and repository state before making changes. Do not repeat completed tasks."`) dispatched automatically upon confirmed quota availability and agent readiness.
   - **Mode 2 (Auto-send)**: Interactive queueing of custom prompts and images with automatic delivery when the agent becomes idle.
6. **Continuous FIFO Queue**: HQ never exits after submitting the first prompt; it immediately accepts the next prompt so you can queue prompts while the agent works.
7. **Process-Pinned Safety**: Automatically pins the target terminal handle and process. Prompts are never silently dispatched to replaced processes or closed panes.
8. **Ultra-Lightweight & Fast**: Single standalone binary written in Rust with zero heavy runtime dependencies and < 5ms startup.

---

## Usage Modes

### 1. Default CLI Mode (`hq` or `hq --cli`)
```bash
# Launch default CLI mode
hq

# Or explicitly
hq --cli

# Launch with an initial prompt
hq "reply with 'hello'"
```

### 2. Interactive TUI Mode (`hq --tui`)
```bash
# Launch interactive TUI mode
hq --tui

# Launch TUI with an initial prompt or image
hq --tui "@/path/to/screenshot.png fix the CSS layout"
```

In TUI mode:
- **Prompt Queue (Left)**: Real-time list of all queued items with status badges (`[⏳ QUEUED]`, `[➤ SENDING]`, `[✓ SENT]`) and image tags (`[🖼️ screenshot.png (142 KB)]`).
- **Event Log (Right)**: Timestamped history of events (`✓ Prompt #1 sent!`, `◷ Waiting for Pi to finish prompt #1...`, `✓ Usage reset detected`).
- **Input Field (Bottom)**: Type new prompts or paste image paths anytime and hit `[Enter]` to enqueue.
- **Detach Cleanly**: Press `[Esc]`, `[Ctrl+C]`, or type `q` to detach safely without killing the agent.

---

## Image Compatibility

When working in terminals like Orca IDE, Herdr, or Ghostty, images can be pasted or typed directly:

```text
› Next prompt (or image path):
@/var/folders/f3/.../orca-paste-1793100415562.png fix the alignment of this button
```

HQ automatically:
1. Detects `@/path/to/image.png` or direct `/path/to/image.png` references.
2. Validates that the file exists and computes its size (e.g. `428 KB`).
3. Embeds the file path and multimodal context into the prompt sent to the agent.
4. Displays `✓ Image attached: [🖼️ Image: orca-paste-1793100415562.png (428.1 KB)]`.

Supported formats: `.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`, `.svg`, `.bmp`, `.tiff`, `.ico`.

---

## Supported Environments & Agents

| Terminal Environment | Detection Method | Prompt Delivery | Notes |
| :--- | :--- | :--- | :--- |
| **Orca IDE** | Native CLI (`orca terminal`) + `TERM_PROGRAM=Orca` | Native IPC (`orca terminal send`) | Auto-detects same-tab splits and active agent sessions. |
| **Herdr** | Herdr CLI (`herdr pane`) + `HERDR_PANE_ID` | Direct agent IPC (`herdr agent prompt`) | Auto-detects adjacent directional panes. |
| **Ghostty** | Ghostty environment + CLI window inspection | Native split pane send | macOS native split panes. |

### Supported Coding Agents

- **OpenAI Codex** (`codex` CLI) — *Full support with structured JSON-RPC quota detection*
- **Claude Code** (`claude` CLI)
- **OpenCode** (`opencode` CLI)
- **OMP** (`omp` CLI)
- **Pi** (`pi` CLI)
- **Google Antigravity** (`agy` / `antigravity`)

---

## Running with Just (Recommended)

You don't need to manually build or install any binary. Use [`just`](https://github.com/casey/just) to run `hq` directly:

```bash
# Launch hq in default CLI mode
just

# Launch hq in interactive TUI mode
just tui

# Run with an initial prompt
just run "Check tests and summarize"

# Run the test suite (58 tests)
just test

# Check lints and types
just check
```

---

## License

MIT / Apache-2.0
