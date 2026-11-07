# Terminal Fallback & Agent Session Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable `hq` to operate without multiplexer panes (e.g. inside Windows Terminal or single panes) by intercepting missing-pane errors, notifying the user, interactively selecting the agent harness and session (`<workspace> '<title>' <timestamp>`), and dispatching prompts directly via a background `FallbackAdapter`.

**Architecture:** A new `session_discovery` module reads local session stores (AGY SQLite DB with bundled `rusqlite`, Codex JSONL index) and formats human-readable session labels. The CLI layer provides an interactive fallback flow when no Herdr/Orca/Ghostty panes are detected. A new `FallbackAdapter` implements `TerminalAdapter` to track turn execution asynchronously and dispatch prompts via the agent's native CLI/session endpoint without stealing mouse or keyboard focus.

**Tech Stack:** Rust 2021, `rusqlite` (bundled feature), `crossterm`, `ratatui`, `serde`, `serde_json`.

**Spec:** [`docs/superpowers/specs/2026-11-07-terminal-fallback-agent-selection-design.md`](file:///Users/romey/Developer/harness-queue/docs/superpowers/specs/2026-11-07-terminal-fallback-agent-selection-design.md)

## Global Constraints

- Use `rusqlite` with `features = ["bundled"]` for zero-system-dependency SQLite support across Windows and macOS.
- Never hijack the user's physical mouse or keyboard focus; dispatch prompts to the agent's native CLI/queue endpoint in the background.
- Do not auto-copy prompts to the system clipboard unless the `--clipboard` flag is passed.
- Session labels must match `<workspace> '<title>' <timestamp>` (e.g. `harness-queue 'adding features in hq' 07/11/2026 11:30pm`).
- Conditional compilation (`#[cfg(unix)]` / `#[cfg(windows)]`) for POSIX-only signal calls in `src/main.rs`.

## Review Focus

1. **Missing or Corrupt SQLite Database:** If AGY's `conversation_summaries.db` does not exist or has an incompatible schema, return an empty session list gracefully without panicking, and allow custom session entry.
2. **Empty or Missing Titles in Sessions:** If a session title is blank in the DB/JSONL, fall back to `"untitled"` so the display format `<workspace> '<title>' <timestamp>` never formats empty quotes or panics.
3. **Turn Asynchrony and Supervisor Flooding:** `FallbackAdapter` must transition `get_agent_status()` to `AgentStatus::Busy` while a turn is executing and back to `AgentStatus::Ready` upon completion, preventing the supervisor loop from dispatching subsequent items concurrently.
4. **Windows Path and Terminal Environment Discovery:** Paths must resolve properly on Windows via `USERPROFILE` or `HOME`, and detect `WT_SESSION` to report `"Windows Terminal (Fallback)"`.
5. **Session Cancel Handling:** In the interactive selection menu, typing `q`, `exit`, or pressing `Ctrl+C` / EOF must cleanly exit without creating a broken supervisor target.

---

### Task 1: Add `rusqlite` and Portability Dependencies to `Cargo.toml`

**Files:**
- Modify: `Cargo.toml:16-21`
- Test: `tests/test_windows_compatibility.rs`

**Interfaces:**
- Consumes: None
- Produces: `rusqlite` crate available with bundled SQLite C engine; platform-safe build configuration.

- [ ] **Step 1: Write the failing test for bundled SQLite availability**

```rust
// tests/test_windows_compatibility.rs
#[test]
fn test_rusqlite_in_memory_connection() {
    let conn = rusqlite::Connection::open_in_memory().expect("in-memory db must open");
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)",
        [],
    ).expect("create table must succeed");
    conn.execute("INSERT INTO test (name) VALUES (?1)", ["hq"]).expect("insert must succeed");
    let name: String = conn.query_row("SELECT name FROM test WHERE id = 1", [], |row| row.get(0)).unwrap();
    assert_eq!(name, "hq");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_windows_compatibility`
Expected: FAIL due to `rusqlite` not in `Cargo.toml` or unknown extern crate.

- [ ] **Step 3: Add `rusqlite = { version = "0.32", features = ["bundled"] }` to `Cargo.toml`**

Update `Cargo.toml` dependencies block.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_windows_compatibility`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock tests/test_windows_compatibility.rs
git commit -m "feat(deps): add rusqlite with bundled feature for cross-platform session discovery"
```

---

### Task 2: Session Discovery Models & Formatters (`src/session_discovery.rs`)

**Files:**
- Create: `src/session_discovery.rs`
- Modify: `src/lib.rs`
- Test: `tests/test_session_discovery.rs`

**Interfaces:**
- Consumes: `crate::models::HarnessKind`
- Produces:
  - `pub struct DiscoveredSession { pub session_id: String, pub title: String, pub workspace: String, pub updated_at: Option<u64>, pub formatted_time: String }`
  - `pub fn format_session_label(session: &DiscoveredSession) -> String`
  - `pub fn format_timestamp(ts_secs: u64) -> String`
  - `pub fn parse_iso_or_sqlite_time(time_str: &str) -> Option<u64>`

- [ ] **Step 1: Write the failing tests for session discovery data models and formatters**

```rust
// tests/test_session_discovery.rs
use harness_queue::session_discovery::{format_session_label, format_timestamp, parse_iso_or_sqlite_time, DiscoveredSession};

#[test]
fn test_format_session_label_exact_style() {
    let session = DiscoveredSession {
        session_id: "conv-123".to_string(),
        title: "adding features in hq".to_string(),
        workspace: "harness-queue".to_string(),
        updated_at: Some(1794051000),
        formatted_time: "07/11/2026 11:30pm".to_string(),
    };
    let label = format_session_label(&session);
    assert_eq!(label, "harness-queue 'adding features in hq' 07/11/2026 11:30pm");
}

#[test]
fn test_format_session_label_empty_title_fallback() {
    let session = DiscoveredSession {
        session_id: "conv-456".to_string(),
        title: "".to_string(),
        workspace: "harness-queue".to_string(),
        updated_at: None,
        formatted_time: "unknown".to_string(),
    };
    let label = format_session_label(&session);
    assert_eq!(label, "harness-queue 'untitled' unknown");
}

#[test]
fn test_parse_sqlite_timestamp() {
    let raw = "2026-11-07 22:30:00+00:00";
    let ts = parse_iso_or_sqlite_time(raw);
    assert!(ts.is_some(), "Should parse sqlite timestamp format");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_session_discovery`
Expected: FAIL with "cannot find module or struct `session_discovery`"

- [ ] **Step 3: Implement `src/session_discovery.rs` and export in `src/lib.rs`**

Implement `DiscoveredSession`, `format_session_label`, `format_timestamp`, and `parse_iso_or_sqlite_time`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_session_discovery`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/session_discovery.rs src/lib.rs tests/test_session_discovery.rs
git commit -m "feat(discovery): add session discovery models and label formatting"
```

---

### Task 3: AGY and Codex Session Readers (`src/session_discovery.rs`)

**Files:**
- Modify: `src/session_discovery.rs`
- Test: `tests/test_session_discovery.rs`

**Interfaces:**
- Consumes: `rusqlite`, `serde_json`, `HarnessKind`
- Produces:
  - `pub fn discover_agy_sessions(db_path: Option<&std::path::Path>) -> Result<Vec<DiscoveredSession>, String>`
  - `pub fn discover_codex_sessions(jsonl_path: Option<&std::path::Path>) -> Result<Vec<DiscoveredSession>, String>`
  - `pub fn discover_sessions_for_harness(kind: &HarnessKind) -> Vec<DiscoveredSession>`
  - `pub fn resolve_home_dir() -> std::path::PathBuf`

- [ ] **Step 1: Write the failing tests for AGY SQLite and Codex JSONL readers**

```rust
// In tests/test_session_discovery.rs
use harness_queue::session_discovery::{discover_agy_sessions, discover_codex_sessions};
use std::io::Write;

#[test]
fn test_discover_agy_sessions_from_sqlite() {
    let tmp_dir = std::env::temp_dir().join(format!("hq_test_agy_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp_dir);
    let db_path = tmp_dir.join("conversation_summaries.db");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "CREATE TABLE conversation_summaries (
            conversation_id TEXT PRIMARY KEY,
            title TEXT,
            workspace_uris TEXT,
            last_modified_time TEXT
        )",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO conversation_summaries VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            "c-uuid-1",
            "adding features in hq",
            "[\"file:///Users/romey/Developer/harness-queue\"]",
            "2026-11-07 23:30:00+00:00"
        ],
    ).unwrap();

    let sessions = discover_agy_sessions(Some(&db_path)).expect("must discover agy sessions");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, "c-uuid-1");
    assert_eq!(sessions[0].title, "adding features in hq");
    assert_eq!(sessions[0].workspace, "harness-queue");
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_discover_codex_sessions_from_jsonl() {
    let tmp_dir = std::env::temp_dir().join(format!("hq_test_codex_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp_dir);
    let jsonl_path = tmp_dir.join("session_index.jsonl");

    let mut file = std::fs::File::create(&jsonl_path).unwrap();
    writeln!(file, r#"{{"id":"codex-1","thread_name":"debug issue","updated_at":"2026-11-07T11:30:00Z"}}"#).unwrap();

    let sessions = discover_codex_sessions(Some(&jsonl_path)).expect("must discover codex sessions");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, "codex-1");
    assert_eq!(sessions[0].title, "debug issue");
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_missing_database_returns_empty_vector_not_error() {
    let missing_path = std::path::Path::new("/nonexistent/conversation_summaries.db");
    let sessions = discover_agy_sessions(Some(missing_path)).unwrap();
    assert!(sessions.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_session_discovery`
Expected: FAIL with functions not found.

- [ ] **Step 3: Implement session readers in `src/session_discovery.rs`**

- Resolve home path checking `HOME` and `USERPROFILE`.
- Query `conversation_summaries` table via `rusqlite`.
- Extract folder basename from `workspace_uris` JSON array.
- Parse `session_index.jsonl` lines for Codex.
- Default to empty vector if files do not exist.
- Implement `discover_sessions_for_harness(kind: &HarnessKind) -> Vec<DiscoveredSession>`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_session_discovery`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/session_discovery.rs tests/test_session_discovery.rs
git commit -m "feat(discovery): implement AGY SQLite and Codex JSONL session readers"
```

---

### Task 4: Implement `FallbackAdapter` (`src/terminal/fallback.rs`)

**Files:**
- Create: `src/terminal/fallback.rs`
- Modify: `src/terminal/mod.rs`
- Test: `tests/test_fallback_adapter.rs`

**Interfaces:**
- Consumes: `crate::terminal::TerminalAdapter`, `crate::models::AgentTarget`
- Produces: `pub struct FallbackAdapter { target: AgentTarget, clipboard_enabled: bool, ... }` implementing `TerminalAdapter`.

- [ ] **Step 1: Write the failing tests for `FallbackAdapter`**

```rust
// tests/test_fallback_adapter.rs
use harness_queue::models::{AgentTarget, HarnessKind};
use harness_queue::terminal::fallback::FallbackAdapter;
use harness_queue::terminal::{AgentStatus, TerminalAdapter};

#[test]
fn test_fallback_adapter_metadata() {
    let target = AgentTarget {
        terminal_id: "standalone".to_string(),
        pane_id: "tab_agent".to_string(),
        pid: 0,
        harness: HarnessKind::Agy,
        session_id: "sess-test".to_string(),
        cwd: "/path/to/project".to_string(),
    };

    let adapter = FallbackAdapter::new(target.clone(), false);
    assert!(adapter.terminal_name().contains("Fallback"));
    assert!(adapter.is_available());
    assert_eq!(adapter.detect_current_pane().unwrap(), "standalone_pane");
    assert_eq!(adapter.get_agent_status("tab_agent").unwrap(), AgentStatus::Ready);
    assert!(adapter.verify_target_alive(&target).unwrap());
}

#[test]
fn test_fallback_adapter_windows_terminal_naming() {
    std::env::set_var("WT_SESSION", "mock-guid-1234");
    let target = AgentTarget {
        terminal_id: "standalone".to_string(),
        pane_id: "tab_agent".to_string(),
        pid: 0,
        harness: HarnessKind::Agy,
        session_id: "sess-test".to_string(),
        cwd: "/path/to/project".to_string(),
    };
    let adapter = FallbackAdapter::new(target, false);
    assert_eq!(adapter.terminal_name(), "Windows Terminal (Fallback)");
    std::env::remove_var("WT_SESSION");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_fallback_adapter`
Expected: FAIL with "cannot find module `fallback`"

- [ ] **Step 3: Implement `src/terminal/fallback.rs` and register in `src/terminal/mod.rs`**

- Track background turn process state using `Arc<Mutex<Option<std::process::Child>>>`.
- If child is active: `try_wait()` to check completion. If still running -> `AgentStatus::Busy`; if completed -> `AgentStatus::Ready`.
- In `send_prompt(&self, target, prompt)`:
  - If AGY: spawn `agy --conversation <session_id> -p <prompt>` (or `--prompt`) in background.
  - If Codex: spawn `codex queue --thread <session_id> --message <prompt>` in background.
  - If `--clipboard` enabled: write to host clipboard (`pbcopy` on macOS, `clip.exe` on Windows).
  - Return `DeliveryAck { acknowledged: true, details: "..." }`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_fallback_adapter`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/terminal/fallback.rs src/terminal/mod.rs tests/test_fallback_adapter.rs
git commit -m "feat(terminal): implement FallbackAdapter with background turn execution"
```

---

### Task 5: Interactive Terminal Agent Selection UI (`src/cli.rs`)

**Files:**
- Modify: `src/cli.rs`
- Test: `tests/test_cli_fallback_selection.rs`

**Interfaces:**
- Consumes: `src/session_discovery.rs`, `crate::models::{AgentTarget, HarnessKind}`
- Produces:
  - `pub fn format_fallback_notification() -> &'static str`
  - `pub fn select_fallback_agent_target<R: BufRead, W: Write>(reader: &mut R, writer: &mut W) -> Option<AgentTarget>`
  - `pub fn select_harness_menu<R: BufRead, W: Write>(reader: &mut R, writer: &mut W) -> Option<HarnessKind>`
  - `pub fn select_session_menu<R: BufRead, W: Write>(reader: &mut R, writer: &mut W, harness: &HarnessKind, sessions: &[DiscoveredSession]) -> Option<AgentTarget>`

- [ ] **Step 1: Write the failing tests for fallback selection menu flow**

```rust
// tests/test_cli_fallback_selection.rs
use harness_queue::cli::{format_fallback_notification, select_fallback_agent_target};
use harness_queue::models::HarnessKind;
use std::io::Cursor;

#[test]
fn test_fallback_notification_content() {
    let note = format_fallback_notification();
    assert!(note.contains("No Herdr, Orca, or Ghostty terminal panes detected"));
    assert!(note.contains("Terminal Agent Selection"));
}

#[test]
fn test_select_fallback_agent_interactive_agy_first_session() {
    // Input: "1" for AGY harness, "1" for first session
    let mut input = Cursor::new(b"1\n1\n");
    let mut output = Vec::new();

    let target = select_fallback_agent_target(&mut input, &mut output).expect("should return an AgentTarget");
    assert_eq!(target.harness, HarnessKind::Agy);
    assert_eq!(target.terminal_id, "standalone");
}

#[test]
fn test_select_fallback_agent_custom_session() {
    // Input: "2" (Codex), "99" (or option for custom session), "custom-thread-id", "custom-cwd"
    let mut input = Cursor::new(b"2\n99\ncustom-session-123\n/tmp/worktree\n");
    let mut output = Vec::new();

    let target = select_fallback_agent_target(&mut input, &mut output).expect("should return an AgentTarget");
    assert_eq!(target.harness, HarnessKind::Codex);
    assert_eq!(target.session_id, "custom-session-123");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_cli_fallback_selection`
Expected: FAIL with functions not found in `src/cli.rs`.

- [ ] **Step 3: Implement selection menus in `src/cli.rs`**

- Implement `format_fallback_notification()`.
- Implement `select_harness_menu()` listing 1) AGY, 2) Codex, 3) Claude Code, 4) OpenCode, 5) OMP, 6) Pi.
- Implement `select_session_menu()` showing discovered sessions using `format_session_label()`, plus `[Enter custom session ID or start new]`.
- Handle cancelling via `q`, `exit`, `Ctrl+C` (returns `None`).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_cli_fallback_selection`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs tests/test_cli_fallback_selection.rs
git commit -m "feat(cli): implement interactive fallback agent and session selection"
```

---

### Task 6: Wire Fallback Selection into `src/main.rs` & Support CLI Flags

**Files:**
- Modify: `src/main.rs`
- Test: `tests/test_all_scenarios.rs`

**Interfaces:**
- Consumes: `detect_terminal_environment`, `FallbackAdapter`, `select_fallback_agent_target`, `format_fallback_notification`
- Produces: Complete end-to-end fallback flow for `hq --tui` and CLI mode.

- [ ] **Step 1: Write integration test for fallback adapter in supervisor flow**

```rust
// In tests/test_all_scenarios.rs
#[test]
fn test_fallback_adapter_supervisor_lifecycle() {
    use harness_queue::models::{AgentTarget, HarnessKind, Mode};
    use harness_queue::supervisor::Supervisor;
    use harness_queue::terminal::fallback::FallbackAdapter;
    use std::sync::Arc;

    let target = AgentTarget {
        terminal_id: "standalone".to_string(),
        pane_id: "tab_agent".to_string(),
        pid: 0,
        harness: HarnessKind::Agy,
        session_id: "sess-agy-fallback".to_string(),
        cwd: "/Users/romey/Developer/harness-queue".to_string(),
    };

    let fallback_terminal = Arc::new(FallbackAdapter::new(target.clone(), false));
    let agy_harness = Arc::from(harness_queue::harness::create_harness_adapter(&HarnessKind::Agy));

    let mut supervisor = Supervisor::new(fallback_terminal.clone(), agy_harness, Mode::AutoSend);
    supervisor.target = Some(target.clone());
    let _ = supervisor.state_machine.transition(harness_queue::models::State::SelectingMode);
    let _ = supervisor.state_machine.transition(harness_queue::models::State::Watching);

    assert_eq!(fallback_terminal.terminal_name(), "Terminal (Fallback)");
    let id = supervisor.enqueue_prompt("test fallback prompt".to_string());
    assert_eq!(id, 1);
}
```

- [ ] **Step 2: Run test to verify it compiles and runs**

Run: `cargo test --test test_all_scenarios test_fallback_adapter_supervisor_lifecycle`
Expected: PASS

- [ ] **Step 3: Update `src/main.rs` to wire fallback flow & flags**

- Check for `--select-agent` flag to force selection.
- Check for `--agent <kind>` flag to pre-select harness.
- Check for `--clipboard` flag.
- When `detect_terminal_environment()` is `None` OR `find_adjacent_agent()` returns `NeighborDetection::NoneFound(_)`:
  - Print `format_fallback_notification()`.
  - Call `select_fallback_agent_target(&mut reader, &mut out)`.
  - If cancelled, exit cleanly with code 0 (`✓ Cancelled`).
  - Create `Arc::new(FallbackAdapter::new(selected_target.clone(), clipboard_flag))`.
  - Continue into TUI (`run_tui(&mut supervisor)`) or CLI mode.
- Gate POSIX signal handling with `#[cfg(unix)]` and provide safe Windows fallback.

- [ ] **Step 4: Run full test suite to verify no regressions**

Run: `cargo test`
Expected: All tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs tests/test_all_scenarios.rs
git commit -m "feat(main): wire fallback terminal agent selection and flags into hq"
```

---

### Task 7: End-to-End Verification & Documentation Updates

**Files:**
- Modify: `README.md`
- Test: Full integration test suite (`cargo test`)

- [ ] **Step 1: Run complete test suite**

Run: `cargo test --all-targets`
Expected: ALL test suites pass (20+ tests).

- [ ] **Step 2: Verify binary builds cleanly with `--release`**

Run: `cargo build --release`
Expected: Successful compile with zero errors.

- [ ] **Step 3: Update `README.md`**

Document the new Terminal Agent Selection fallback, Windows Terminal support, `--select-agent`, `--agent`, and `--clipboard` flags.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: document terminal fallback agent selection and Windows Terminal support"
```
