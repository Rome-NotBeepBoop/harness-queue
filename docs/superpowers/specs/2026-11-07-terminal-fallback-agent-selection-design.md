# Design Document: Terminal Fallback & Agent Session Selection

- **Date:** 2026-11-07
- **Author:** Antigravity / Romey
- **Topic:** Cross-platform terminal fallback, interactive agent & session selection, and background prompt dispatch for Harness Queue (`hq`)

---

## 1. Intent and The Why

Currently, `hq` (`harness-queue`) strictly requires terminal split-pane multiplexing environments—namely Orca IDE, Herdr, or Ghostty macOS AppleScript. If `hq` is invoked in an unsupported terminal or inside a standalone tab/pane without an adjacent agent (such as Windows Terminal on Windows, or standard single-window terminal emulators), `detect_terminal_environment()` or `find_adjacent_agent()` returns an error and immediately aborts execution.

This change enables `hq` to run seamlessly in any standalone terminal pane—most notably Windows Terminal on Windows—by introducing:
1. Automatic detection failure interception that alerts the user and smoothly falls back to **Terminal Agent Selection**.
2. An interactive two-step selection menu:
   - Select agent harness (AGY, Codex, Claude Code, OpenCode, OMP, Pi).
   - Select active/recent session with workspace name, session title, and timestamp (e.g. `harness-queue 'adding features in hq' 07/11/2026 11:30pm`).
3. A cross-platform `FallbackAdapter` implementing `TerminalAdapter` that detects whether an agent is running in another tab/process, tracks turn execution asynchronously, and dispatches prompts directly into the agent's native CLI/session queue endpoint without taking over the user's mouse and physical keyboard.
4. Seamless integration with the existing `hq --tui` and CLI workflows.

---

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
- **Fallback on Missing Multiplexer:** If no Herdr, Orca, or Ghostty panes are detected when invoking `hq --tui` (or `hq`), notify the user with a clean prompt and switch to Terminal Agent Selection rather than exiting.
- **Harness & Session Picker:**
  - Let the user pick from supported harnesses (AGY, Codex, Claude Code, OpenCode, OMP, Pi).
  - Discover sessions from each agent's local store:
    - AGY: `conversation_summaries.db` queried via `rusqlite` (bundled).
    - Codex: `session_index.jsonl`.
    - Other agents: running process detection (PID + cwd) or manual session entry.
  - Display format: `<workspace_folder> '<session_title>' <timestamp (MM/DD/YYYY hh:mma)>`.
  - Allow entering a custom/new session ID if none are found or if the user desires a new session.
- **Non-Intrusive Prompt Delivery:**
  - Locate running agent processes in background tabs/windows cross-platform (`ps` on macOS/Linux, `tasklist` / PowerShell on Windows).
  - Deliver prompts through the agent's native background CLI/session queue endpoint (e.g., `agy --conversation <id> -p ...`, `codex queue --thread <id> --message ...`).
  - Zero disruption: never move the mouse, never hijack physical keyboard focus, and never simulate global key events.
  - Do not auto-copy to the clipboard by default (support optional `--clipboard` flag if user desires).
- **Asynchronous Execution & Readiness Tracking:** `FallbackAdapter` tracks active turn child processes so the supervisor loop correctly transitions through `AgentBusy` -> `AgentReady` without freezing the TUI or flooding turns.
- **Full TUI Parity:** Once configured, the standard `hq --tui` runs identically, displaying the fallback host, target agent, queue state, and logs.
- **Windows Portability:** Ensure paths (`USERPROFILE` / `HOME`), process queries, SQLite queries via bundled `rusqlite`, and signal handling compile and execute on Windows.

### Non-Goals
- Attempting to control proprietary terminal window layouts on Windows Terminal via private Windows APIs.
- Replacing Herdr or Orca split pane detection when those multiplexers are actually present.

### Anti-Goals
- **No Global Mouse or Keyboard Hijacking:** Do NOT use synthetic global mouse clicks, screen drag events, or foreground active window hijacking. Delivery must occur via background process IPC / CLI.
- **No Clipboard Overwrites by Default:** Do not overwrite the host clipboard automatically during background runs.
- **No Silent Hanging:** If no sessions or processes are found, always give clear guidance and an option to specify a session manually or cancel cleanly (`Ctrl+C` or `q`).

---

## 3. Constraints
- **Self-Contained Dependencies:** Use `rusqlite` with the `bundled` feature in `Cargo.toml` so SQLite database parsing works out-of-the-box on Windows without requiring an external `sqlite3` CLI or preinstalled dynamic libraries.
- **Cross-Platform Compatibility:** Must build and run on both Windows (Windows Terminal / PowerShell / CMD) and macOS/Linux.
- **POSIX Signal Safety:** Signal handling in `src/main.rs` currently uses `libc::signal`. For Windows, ensure conditional compilation (`#[cfg(unix)]` / `#[cfg(windows)]`) so Windows builds do not fail on POSIX symbols.

---

## 4. Approaches Considered

### Approach 1: Hybrid Agent IPC & Process Bridge (Selected)
- Intercept pane detection failures, display a clear notice, and run the interactive harness & session picker.
- Query local session stores (`conversation_summaries.db` for AGY, `session_index.jsonl` for Codex) for session titles, workspaces, and timestamps.
- Dispatches prompts via background native agent CLI/queue (`agy --conversation ... -p ...`, `codex queue ...`), tracking execution asynchronously without moving mouse or hijacking focus.
- **Why chosen:** Clean, rock-solid, completely background, works on Windows and macOS, completely avoids brittle GUI hooks.

### Approach 2: Virtual Terminal Input Injection (Rejected)
- Inject synthetic `WM_CHAR` or Accessibility keystrokes into the Windows Terminal background tab.
- **Why rejected:** Windows Terminal uses XAML island rendering and DirectWrite buffers; background synthetic window messages are frequently ignored or dropped when tabs lose focus, causing missed or truncated characters.

### Approach 3: Background Stdio / IPC Daemon Pipe (Rejected)
- Run a custom persistent background daemon for each agent tab with a named pipe.
- **Why rejected:** Requires the user to start agents inside a special wrapper rather than just opening their usual terminal.

---

## 5. Detailed Technical Design

### 5.1. Session Discovery Module (`src/session_discovery.rs`)
A dedicated module to query agent session metadata:
```rust
pub struct DiscoveredSession {
    pub session_id: String,
    pub title: String,
    pub workspace: String,
    pub updated_at: Option<u64>, // Unix timestamp
    pub formatted_time: String,  // e.g. "07/11/2026 11:30pm"
}
```
- **AGY (Google Antigravity):**
  - Path: `~/.gemini/antigravity/conversation_summaries.db` (resolves `HOME` or `USERPROFILE`).
  - Queries `conversation_summaries` table via `rusqlite` (bundled):
    `SELECT conversation_id, title, workspace_uris, last_modified_time FROM conversation_summaries ORDER BY last_modified_time DESC LIMIT 20`
  - Formats timestamp to `MM/DD/YYYY hh:mma`.
  - Extracts workspace directory name from `workspace_uris` (e.g. `file:///.../harness-queue` -> `harness-queue`).
- **Codex (OpenAI Codex):**
  - Path: `~/.codex/session_index.jsonl`.
  - Reads entries `{"id": "...", "thread_name": "...", "updated_at": "..."}`.
- **Claude Code / OpenCode / OMP / Pi:**
  - Scans running processes on the system to find active agent instances (PID and working directory).
- **Fallback / Manual Entry:**
  - Every agent menu includes an option: `[Enter custom session ID or start new]`.

### 5.2. Interactive Terminal Agent Selection (`src/cli.rs`)
1. **Notice:**
   ```text
   ! No Herdr, Orca, or Ghostty terminal panes detected.
   › Falling back to Terminal Agent Selection...
   ```
2. **Harness Selection Menu:**
   ```text
   ? Select agent harness:
     1) Google Antigravity (AGY)
     2) OpenAI Codex
     3) Claude Code
     4) OpenCode
     5) OMP
     6) Pi
   › Choice [1]: 
   ```
3. **Session Selection Menu:**
   ```text
   ? Select session for Google Antigravity (AGY):
     1) harness-queue 'adding features in hq' 07/11/2026 11:30pm
     2) TerminalFolder 'update styles' 07/11/2026 10:15am
     3) [Enter custom session ID or start new]
   › Select session [1]: 
   ```
4. If option 3 is picked, prompts for session ID / name.
5. Produces an `AgentTarget` configured with:
   - `terminal_id: "standalone"`
   - `pane_id: "tab_agent"`
   - `harness: HarnessKind`
   - `session_id: <selected_session_id>`
   - `cwd: <workspace_path>`

### 5.3. Fallback Terminal Adapter (`src/terminal/fallback.rs`)
Implements `TerminalAdapter`:
- `terminal_name()`:
  - If `std::env::var("WT_SESSION").is_ok()` or on Windows, returns `"Windows Terminal (Fallback)"`.
  - Otherwise returns `"Terminal (Fallback)"`.
- `is_available()`: Returns `true`.
- `detect_current_pane()`: Returns `Ok("standalone_pane".to_string())`.
- `find_adjacent_agent()`: Returns `Ok(NeighborDetection::Exact(target))`.
- `verify_target_alive()`:
  - If a process PID is known, checks if the PID is still alive. If headless session CLI, returns `true` as long as the session target is reachable.
- `get_agent_status()`:
  - Checks if a background turn process is currently running:
    - If running: returns `AgentStatus::Busy`.
    - If finished: returns `AgentStatus::Ready`.
    - If no turn active: returns `AgentStatus::Ready`.
- `send_prompt()`:
  - Spawns the background turn process:
    - **AGY:** `agy --conversation <session_id> --print <prompt>`
    - **Codex:** `codex queue --thread <session_id> --message <prompt>`
    - **Other agents:** spawns their CLI turn command in background or dispatches via IPC socket.
  - If `--clipboard` flag is active, updates host clipboard without stealing focus.
  - Returns `Ok(DeliveryAck { acknowledged: true, details: "Dispatched via background agent CLI" })`.
- `get_agent_preview()`: Returns recent log output or status preview if available.

### 5.4. Integration in `main.rs` & CLI Options
- When `detect_terminal_environment()` is `None` OR `find_adjacent_agent()` returns `NoneFound`:
  - Show the fallback notification.
  - Run the `select_fallback_agent_target()` interactive flow.
  - Instantiate `Arc::new(FallbackAdapter::new(target.clone()))`.
  - Proceed into `run_tui(&mut supervisor)` (for `--tui`) or default CLI mode.
- Add CLI flag support:
  - `hq --select-agent`: Force terminal agent selection even if panes exist.
  - `hq --agent <agy|codex|claude|...>`: Pre-select harness for quick session picking.
  - `hq --clipboard`: Enable copying dispatched prompts to the system clipboard.

---

## 6. What's Left to the Builder
- Spacing and numbering alignment in the CLI selection lists.
- Timezone and formatting edge cases for ISO-8601 vs SQLite UTC strings in timestamps.
- Exact command argument configurations for Claude Code / OpenCode CLI runners.
