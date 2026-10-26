# Harness Queue (`hq`)

Minimal, universal, terminal-based agent queue that works alongside existing coding-agent harnesses inside **Herdr panes**.

```text
❯ hq

✓ Herdr detected
✓ Adjacent agent: OpenAI Codex
  Session 7f2a · pane w1:p3

Harness Queue v0.1

? Select mode:

  1) Auto-resume
     Resume with the built-in continuation prompt.

  2) Auto-send
     Queue your own prompts for automatic delivery.

› Select [1]:
```

## Features

- **Zero wrappers required**: Works with agents already launched in an adjacent Herdr pane (OpenAI Codex, Claude Code, OpenCode, OMP, Pi, AGY).
- **Two mutually exclusive modes**:
  - **Mode 1 (Auto-resume)**: Predefined prompt (`"Continue the unfinished work. Inspect the session history and repository state before making changes. Do not repeat completed tasks."`) dispatched automatically upon confirmed quota availability and agent readiness.
  - **Mode 2 (Auto-send)**: Interactive queueing of custom prompts with automatic delivery when the agent becomes idle.
- **Continuous queueing**: HQ never exits after submitting the first prompt; it presents the next prompt prompt input immediately while the agent works in the background.
- **Pinned target safety**: Tracks agent target PID and pane ID to ensure prompts are never silently dispatched to replaced processes or closed panes.
- **Quota awareness**: Directly inspects structured provider quota endpoints (`codex app-server` JSON-RPC) to distinguish approval prompts from genuine usage rate limits.
- **Fast & lightweight**: Standalone single-binary executable compiled with Rust (~700 KB, zero runtime dependencies, < 5ms startup).

## Installation & Build

Requires Rust / Cargo (1.80+):

```bash
# Build release executable
cargo build --release

# Install to ~/.local/bin or /usr/local/bin
cp target/release/hq ~/.local/bin/
```

## Usage

1. Open a split pane in Herdr:
   ```bash
   herdr pane split --current --direction right
   ```
2. In the right pane, launch your agent normally (e.g. `codex`).
3. In the left pane, run:
   ```bash
   hq
   ```
4. HQ automatically detects Herdr, pairs with the adjacent agent, and prompts for mode selection:
   - Press `1` (or Enter) for **Auto-resume**.
   - Press `2` for **Auto-send**, then enter your prompt.
5. As soon as the first prompt is dispatched (`✓ Prompt #1 sent!`), HQ presents `› Next prompt:`. You can queue subsequent prompts anytime.

## Keyboard Controls & Detach

- **Ctrl+C**: Detaches HQ cleanly without stopping or interrupting the agent.

## Automated Test Suite

Run the full test suite covering all 20 required verification scenarios:

```bash
cargo test
```
