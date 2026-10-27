# Harness Queue (`hq`)

Minimal, universal, terminal-based agent queue that works alongside existing coding-agent harnesses inside **Orca IDE** and **Herdr** terminal panes.

```text
┌──────────────────────────────────────────────────────────┐
│  Harness Queue (hq) v0.1                                 │
│  Host: Orca IDE · Location: term_d5d8770b                │
│  Target: OpenAI Codex (term_7b2d3b68)                    │
└──────────────────────────────────────────────────────────┘

? Select mode:
  1) Auto-resume  (default — press [Enter])
     Resume with the built-in continuation prompt.

  2) Auto-send
     Queue your own prompts for automatic delivery.

› Choice [1] or enter prompt directly: 
```

## Core Philosophy & Design

HQ is designed to feel like a tiny, unobtrusive addition to your existing terminal workflow:

1. **Dual Terminal Auto-Detection**: Works seamlessly inside **Orca IDE** terminal splits and **Herdr** panes with zero manual configuration.
2. **Pure Open-Source CLI & IPC**: **Zero AppleScript**, zero accessibility hacks, zero synthetic mouse movements. Built purely with native CLI and JSON-RPC APIs for full open-source portability.
3. **Zero Wrappers Required**: Launch your agent normally (OpenAI Codex, Claude Code, OpenCode, OMP, Pi, Google Antigravity). HQ automatically discovers and pairs with the neighboring agent.
4. **Intuitive 0-Learning-Curve UX**:
   - Press **`[Enter]`** to start Auto-resume immediately.
   - Or **type your prompt directly** to queue Prompt #1 without navigating extra menus.
   - Or run **`hq "your prompt"`** from the command line to start queueing in one step.
5. **Two Mutually Exclusive Modes**:
   - **Mode 1 (Auto-resume)**: Predefined prompt (`"Continue the unfinished work. Inspect the session history and repository state before making changes. Do not repeat completed tasks."`) dispatched automatically upon confirmed quota availability and agent readiness.
   - **Mode 2 (Auto-send)**: Interactive queueing of custom prompts with automatic delivery when the agent becomes idle.
6. **Continuous FIFO Queue**: HQ never exits after submitting the first prompt; it immediately presents the next prompt input so you can queue prompts while the agent works.
7. **Process-Pinned Safety**: Automatically pins the target terminal handle and process. Prompts are never silently dispatched to replaced processes or closed panes.
8. **Ultra-Lightweight & Fast**: Single standalone binary written in Rust (~700 KB, zero runtime dependencies, < 5ms startup).

---

## Supported Environments & Agents

| Terminal Environment | Detection Method | Prompt Delivery | Notes |
| :--- | :--- | :--- | :--- |
| **Orca IDE** | Native CLI (`orca terminal`) + `TERM_PROGRAM=Orca` | Native IPC (`orca terminal send`) | Auto-detects same-tab splits and active agent sessions. |
| **Herdr** | Herdr CLI (`herdr pane`) + `HERDR_PANE_ID` | Direct agent IPC (`herdr agent prompt`) | Auto-detects adjacent directional panes (right/left/down/up). |

### Supported Coding Agents

- **OpenAI Codex** (`codex` CLI) — *Full support with structured JSON-RPC quota detection*
- **Claude Code** (`claude` CLI)
- **OpenCode** (`opencode` CLI)
- **OMP** (`omp` CLI)
- **Pi** (`pi` CLI)
- **Google Antigravity** (`agy` / `antigravity`)

---

## Installation & Build

### Pre-requisites
- Rust 1.80+ (`cargo`)

### Build Release Binary

```bash
# Clone the repository
git clone https://github.com/your-username/harness-queue.git
cd harness-queue

# Compile optimized release executable
cargo build --release

# Install to ~/.local/bin or /usr/local/bin
cp target/release/hq ~/.local/bin/
```

---

## Quickstart

### In Orca IDE

1. Open your project in Orca IDE.
2. In one terminal pane, launch your coding agent (e.g. `codex`).
3. Split the terminal pane (`⌘D` or split button) and run:
   ```bash
   hq
   ```
4. HQ automatically detects Orca IDE and pairs with your adjacent agent:
   - Hit **`[Enter]`** to auto-resume with the continuation prompt.
   - Or type a custom prompt and hit Enter.

### In Herdr

1. Open a split pane in Herdr:
   ```bash
   herdr pane split --current --direction right
   ```
2. In the right pane, launch your agent normally (e.g. `codex`).
3. In the left pane, run:
   ```bash
   hq
   ```

### Power-User Command-Line Launch

You can supply your prompt directly on the command line to start queueing instantly:

```bash
hq "Implement receiving webhook handler and run unit tests"
```

---

## Continuous Queueing Workflow

After the first prompt is sent, HQ immediately presents the next prompt prompt:

```text
✓ Prompt #1 sent!

› Next prompt:
Update inventory import UI to show progress bar.

✓ Prompt #2 queued
◷ Waiting for Codex to finish...

✓ Codex finished prompt #1
➤ Sending prompt #2...

✓ Prompt #2 sent!

› Next prompt:
```

- Prompts remain queued while the agent is busy or rate-limited.
- Dispatched automatically as soon as the agent becomes ready.
- Press **Ctrl+C** anytime to detach HQ cleanly without interrupting active agent turns.

---

## Automated Test Suite

HQ comes with an exhaustive test suite covering unit tests and 20 end-to-end integration scenarios:

```bash
cargo test
```

All 48 tests run and pass in under 2 seconds.

---

## License

MIT / Apache 2.0
