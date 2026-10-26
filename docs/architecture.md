# Architecture of Harness Queue (`hq`)

## Overview

Harness Queue (`hq`) is architected around four decoupled layers:

```
┌────────────────────────────────────────────────────────┐
│                      CLI & UX                          │
│        Interactive Stdin/Stdout, Mode Selection         │
└───────────────────────────┬────────────────────────────┘
                            │
┌───────────────────────────▼────────────────────────────┐
│                    Supervisor                          │
│   Continuous Queue (FIFO) + Explicit State Machine     │
└─────────────┬────────────────────────────┬─────────────┘
              │                            │
┌─────────────▼─────────────┐┌─────────────▼─────────────┐
│     Terminal Adapters     ││      Harness Adapters     │
│  - Herdr CLI / Socket     ││  - Codex (App-Server RPC) │
│  - Ghostty (AppleScript)  ││  - Claude Code            │
│  - Mock Adapter           ││  - OpenCode / OMP / Pi    │
└───────────────────────────┘└───────────────────────────┘
```

## Core Components

### 1. Terminal Adapter (`src/terminal/`)
Abstracted behind the `TerminalAdapter` trait:
- `detect_current_pane()`: Identifies the caller's pane (using `HERDR_PANE_ID` in Herdr or working directory / TTY in Ghostty).
- `find_adjacent_agent()`: Searches topological neighbors (right, left, down, up) and extracts process trees (`pane process-info`).
- `get_agent_status()`: Queries agent lifecycle state (`idle`, `working`, `blocked`, `done`, `unknown`).
- `send_prompt()`: Submits prompts through high-level agent APIs (`herdr agent prompt`) or terminal input buffers (`input text` + `send key enter`).
- `verify_target_alive()`: Pinning check comparing current PID to the initial paired PID.

### 2. Harness Adapter (`src/harness/`)
Abstracted behind the `HarnessAdapter` trait:
- **OpenAI Codex**: Full integration via `codex app-server --listen stdio://` JSON-RPC (`account/rateLimits/read` and `thread/list`). Distinguishes 100% quota depletion, `rateLimitReachedType`, and `spendControlReached` from human approval prompts.
- **Claude Code, OpenCode, OMP, Pi, AGY**: Progressive adapters that track native sessions and deliver queued prompts via terminal and CLI interfaces.

### 3. State Machine & Continuous Queue (`src/state_machine.rs`, `src/queue.rs`)
- Explicit states: `Detecting`, `SelectingMode`, `Watching`, `RateLimited`, `WaitingForRecovery`, `RecoveringSession`, `AgentBusy`, `AgentReady`, `Submitting`, `PromptAcknowledged`, `DeliveryUncertain`, `Error`, `Detached`.
- Queue items track distinct states (`Queued`, `Submitting`, `Acknowledged`, `Uncertain`).
- Prevents double submissions or dispatch while the agent is busy or rate-limited.
