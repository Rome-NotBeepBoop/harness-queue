# Support Matrix

Harness Queue (`hq`) supports progressive compatibility across terminal multiplexers and coding agent harnesses.

## Terminal Adapters

| Terminal | Compatibility | Neighbor Discovery | Input Mechanism | Status Detection | Notes |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Orca IDE** | **Full** | Native CLI (`orca terminal list --json`), same-tab split prioritization | Native IPC (`orca terminal send --enter --json`) | Native (`preview`, `connected`, `writable`) | First-class IDE terminal support. Auto-detects same-tab splits and active agent identity. |
| **Herdr** | **Full** | Native (`herdr pane neighbor`, `layout`) | Native (`herdr agent prompt`, `pane send-text`) | Native (`herdr agent get`, `process-info`) | Directional neighbor discovery (right, left, down, up) and agent IPC. |
| **Mock** | **Full** | Simulated topology | In-memory atomic queue | State simulation | Used by automated test suite for headless verification. |

## Harness Adapters

| Harness | Level | Session Tracking | Quota Inspection | Automatic Continuation | Notes |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **OpenAI Codex** | **Full** | Yes (`thread/list`, session files) | Yes (`codex app-server` JSON-RPC `account/rateLimits/read`) | Yes (`codex queue` & agent prompt) | Full structured rate-limit windows (`resetsAt`, `usedPercent`, spend controls). |
| **Claude Code** | **Partial** | Yes (session files in `~/.claude/sessions`) | Fallback (observed output) | Yes (terminal prompt injection) | Terminal hook & process discovery. |
| **OpenCode** | **Partial** | Yes (session DB & config) | Fallback (observed output) | Yes (terminal prompt injection) | Discovered via `opencode` command line & process tree. |
| **OMP** | **Partial** | Yes (profile directory) | Fallback (observed output) | Yes (terminal prompt injection) | Discovered via `omp` command line & process tree. |
| **Pi** | **Partial** | Yes (`~/.pi/sessions`) | Fallback (observed output) | Yes (terminal prompt injection) | Discovered via `pi` command line & process tree. |
| **Google Antigravity (`agy`)** | **Partial** | Yes (transcript directory) | Fallback (observed output) | Yes (terminal prompt injection) | Discovered via `agy` command line & process tree. |
