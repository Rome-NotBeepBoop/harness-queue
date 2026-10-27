# Architecture of Harness Queue (`hq`)

## Overview

Harness Queue (`hq`) is architected around four decoupled layers:

```
┌────────────────────────────────────────────────────────┐
│                      CLI & UX                          │
│     Framed Box Header, Mode Selection, Prompt Input    │
└───────────────────────────┬────────────────────────────┘
                            │
┌───────────────────────────▼────────────────────────────┐
│                    Supervisor                          │
│   Continuous Queue (FIFO) + Explicit State Machine     │
└─────────────┬────────────────────────────┬─────────────┘
              │                            │
┌─────────────▼─────────────┐┌─────────────▼─────────────┐
│     Terminal Adapters     ││      Harness Adapters     │
│  - Orca IDE (CLI / IPC)   ││  - Codex (App-Server RPC) │
│  - Herdr (CLI / Socket)   ││  - Claude Code            │
│  - Mock Adapter           ││  - OpenCode / OMP / Pi    │
│                           ││  - Google Antigravity     │
└───────────────────────────┘└───────────────────────────┘
```

## Core Components

### 1. Terminal Adapters (`src/terminal/`)
Abstracted behind the `TerminalAdapter` trait:
- `detect_current_pane()`: Identifies the caller's pane (`ORCA_TERMINAL_HANDLE` or `HERDR_PANE_ID`).
- `find_adjacent_agent()`: Discovers adjacent agents:
  - In **Orca IDE**: Scans active terminals using `orca terminal list --json`, prioritizing the caller's split tab (`tabId == ORCA_TAB_ID`), then matching active agent identities and worktrees, filtering out orphaned terminals.
  - In **Herdr**: Searches topological neighbors (right, left, down, up) via `herdr pane neighbor` and extracts process trees (`pane process-info`).
- `get_agent_status()`: Queries agent lifecycle and readiness state (`Ready`, `Busy`, `BlockedApproval`, `RateLimited`, `Unknown`).
- `send_prompt()`: Submits prompts through native CLI/IPC:
  - In Orca: `orca terminal send --terminal <handle> --text <prompt> --enter --json`
  - In Herdr: `herdr agent prompt --pane <pane_id> --prompt <prompt>`
- `verify_target_alive()`: Pinning check ensuring target terminal handle / PID is still alive and connected.

> **Zero Workarounds**: Built purely using open-source CLI and JSON-RPC endpoints. No AppleScript, macOS Accessibility permissions, or synthetic cursor manipulation.

### 2. Harness Adapters (`src/harness/`)
Abstracted behind the `HarnessAdapter` trait:
- **OpenAI Codex**: Full integration via `codex app-server --listen stdio://` JSON-RPC (`account/rateLimits/read` and `thread/list`). Distinguishes quota depletion, `rateLimitReachedType`, and `spendControlReached` from human approval prompts.
- **Claude Code, OpenCode, OMP, Pi, AGY**: Progressive adapters that track native sessions and deliver queued prompts via terminal and CLI interfaces.

### 3. State Machine & Continuous Queue (`src/state_machine.rs`, `src/queue.rs`)
- Explicit states: `Detecting`, `SelectingMode`, `Watching`, `RateLimited`, `WaitingForRecovery`, `RecoveringSession`, `AgentBusy`, `AgentReady`, `Submitting`, `PromptAcknowledged`, `DeliveryUncertain`, `Error`, `Detached`.
- Queue items track distinct states (`Queued`, `Submitting`, `Acknowledged`, `Uncertain`).
- Prevents double submissions or dispatch while the agent is busy or rate-limited.
- FIFO ordering strictly preserved.
