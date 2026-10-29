# UI Enhancements, Usage Display & TUI/CLI Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the TUI Session Overview container cutoff on narrow panes, add agent usage tracking for Codex, Claude Code, and AGY (only shown when detected), prompt users to disambiguate multiple agents in the same tab in both TUI and CLI, and achieve full feature parity between CLI and TUI.

**Architecture:** 
- Add `get_usage_summary` to `HarnessAdapter` trait and `get_agent_preview` to `TerminalAdapter` trait; implement usage extractors in `CodexHarness` (JSON-RPC rate limits & quota reset), `ClaudeHarness` (terminal preview & status), and `AgyHarness` (quota & session state); leave all other harnesses as `None`.
- Restructure TUI Session Overview in `src/tui/app.rs` with expanded container height (7-8 lines), text wrapping (`Wrap { trim: true }`), and split fields across dedicated rows to eliminate cutoff on narrow splits.
- Unify multi-agent detection in `src/main.rs` so both CLI and TUI present the user with an explicit selection menu displaying all detected agents in the tab.
- Update CLI boxed header and runtime commands in `src/cli.rs` and `src/main.rs` to display worktree, agent usage, and queue inspection, matching TUI capabilities.

**Tech Stack:** Rust 2021, Ratatui 0.29, Crossterm 0.28, Serde JSON, Libc.

**Spec:** Direct user requirements from turn 10:
1. Fix Session Overview container cutoff in TUI: extend container downward and prevent truncation of Mode, Status, Worktree on narrow split panes.
2. Show Codex, Claude Code, and AGY usages on Session Overview, but only if that specific agent is detected on the same terminal as harness-queue.
3. If multiple agents exist on the same terminal tab (e.g., two AGY agents), explicitly show the user all detected agents in the tab and give the option to choose between them before launching (in both TUI and CLI).
4. Full feature parity between CLI and TUI (and vice-versa).

## Global Constraints
- Target agents must be within the exact same folder and same terminal tab/split-pane.
- Never hijack the physical desktop mouse or use AppleScript global clicks.
- `HarnessAdapter::get_usage_summary` must return `None` for Pi, OpenCode, OMP, and Unknown harnesses.
- Must compile cleanly with `cargo clippy --all-targets -- -D warnings` and pass all unit tests.

## Review Focus
1. Narrow terminal panes (< 50 columns): ensure all Session Overview fields (Host, Mode, Status badge, Target, Worktree, and Usage) render without text cutoff or panic.
2. Non-supported harnesses (Pi, OpenCode, OMP): verify that no usage line is displayed.
3. Ambiguous agent selection in TUI mode: verify TUI mode never silently selects `list[0]` and always prompts user choice.
4. Codex quota reset calculation: handle epoch timestamps in the future and past cleanly without integer underflow.
5. Interactive raw mode in CLI: verify that `queue` and `status` commands render without disrupting raw mode or cursor position.

---

### Task 1: Add Usage Extraction to `HarnessAdapter` & Implement for Codex, Claude Code, and AGY

**Files:**
- Modify: `src/terminal/mod.rs`
- Modify: `src/terminal/orca.rs`
- Modify: `src/terminal/herdr.rs`
- Modify: `src/terminal/ghostty.rs`
- Modify: `src/harness/mod.rs`
- Modify: `src/harness/codex.rs`
- Modify: `src/harness/claude.rs`
- Modify: `src/harness/agy.rs`
- Modify: `src/supervisor.rs`
- Test: `tests/usage_test.rs`

**Interfaces:**
- `TerminalAdapter::get_agent_preview(&self, pane_id: &str) -> Option<String>`
- `HarnessAdapter::get_usage_summary(&self, target: &AgentTarget, terminal: &dyn TerminalAdapter) -> Option<String>`
- `Supervisor::get_target_usage_summary(&self) -> Option<String>`

- [ ] **Step 1: Write failing unit test for usage summaries**

Create `tests/usage_test.rs` testing that `CodexHarness`, `ClaudeHarness`, and `AgyHarness` return `Some(...)` usage summaries, while `PiHarness` and `OpenCodeHarness` return `None`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test usage_test`
Expected: FAIL with missing method `get_usage_summary`.

- [ ] **Step 3: Add `get_agent_preview` to `TerminalAdapter` and implement in terminal adapters**

In `src/terminal/mod.rs`:
```rust
fn get_agent_preview(&self, _pane_id: &str) -> Option<String> {
    None
}
```
In `src/terminal/orca.rs`: implement `get_agent_preview` looking up `entry.preview`.
In `src/terminal/herdr.rs` and `src/terminal/ghostty.rs`: provide sensible preview or None.

- [ ] **Step 4: Add `get_usage_summary` to `HarnessAdapter` and implement for Codex, Claude, and AGY**

In `src/harness/mod.rs`:
```rust
fn get_usage_summary(&self, _target: &AgentTarget, _terminal: &dyn TerminalAdapter) -> Option<String> {
    None
}
```
In `src/harness/codex.rs`:
Extract `primary.usedPercent`, `primary.resetsAt`, and format:
`Codex Quota: {used:.1}% used (5h reset: {hours}h {mins}m)`.
In `src/harness/claude.rs`:
Inspect status and preview to format:
`Claude Code: Active · Ready (no limit observed)` or `Claude Code: Rate limit observed`.
In `src/harness/agy.rs`:
Inspect status and preview to format:
`Antigravity: Active · Quota available` or `Antigravity: Rate limited`.
In `src/supervisor.rs`:
Add `pub fn get_target_usage_summary(&self) -> Option<String>`.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --test usage_test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/terminal/ src/harness/ src/supervisor.rs tests/usage_test.rs
git commit -m "feat(harness): add usage summary extraction for Codex, Claude Code, and AGY"
```

---

### Task 2: Fix TUI Session Overview Layout & Height (No More Cutoffs)

**Files:**
- Modify: `src/tui/app.rs`
- Test: `tests/tui_test.rs`

**Interfaces:**
- `TuiApp` header rendering uses expanded height constraint and wraps lines cleanly.

- [ ] **Step 1: Write test for TUI layout constraints**

Verify in a unit test that `TuiApp` layout calculates header height dynamically (e.g. 7 or 8) and doesn't exceed bounds.

- [ ] **Step 2: Update `src/tui/app.rs` layout and line structure**

In `src/tui/app.rs`:
1. Calculate header height dynamically:
```rust
let usage_line = app.supervisor.get_target_usage_summary();
let header_height = if usage_line.is_some() { 8 } else { 7 };
```
2. Split header fields across logical lines:
- Line 1: `Harness Queue (hq) v0.1`  •  Host: `Orca IDE`
- Line 2: Mode: `Auto-send`  •  Status: `state_badge`
- Line 3: Target: `target.harness.display_name()` (`target.short_session_id()`)
- Line 4: Worktree: `target.cwd`
- Line 5 (optional): Usage: `usage_line`
3. Add `.wrap(ratatui::widgets::Wrap { trim: true })` to the header Paragraph widget.

- [ ] **Step 3: Run cargo test to verify compilation and tests**

Run: `cargo test --test tui_test`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/tui/app.rs tests/tui_test.rs
git commit -m "fix(tui): expand Session Overview container and prevent field truncation on narrow panes"
```

---

### Task 3: Multi-Agent Disambiguation in Both CLI and TUI

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/main.rs`
- Test: `tests/disambiguation_test.rs`

**Interfaces:**
- `select_ambiguous_target` explicitly lists all candidate agents with their terminal ID and worktree.
- `main.rs` invokes `select_ambiguous_target` for both CLI and TUI modes when `NeighborDetection::Ambiguous(list)` is returned.

- [ ] **Step 1: Write failing test for ambiguous target selection**

Create `tests/disambiguation_test.rs` testing `select_ambiguous_target` when given two AGY agents in the same tab.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test disambiguation_test`

- [ ] **Step 3: Enhance `select_ambiguous_target` in `src/cli.rs`**

Format each target with harness name, pane id, and worktree:
```text
? Multiple agents detected in this terminal tab (2 found):
  1) Google Antigravity (term_bbda9ebd) · worktree: /Users/romey/Developer/harness-queue
  2) Google Antigravity (term_743d09f8) · worktree: /Users/romey/Developer/harness-queue
› Select agent to target [1]: 
```

- [ ] **Step 4: Update `src/main.rs` to call `select_ambiguous_target` in both TUI and CLI modes**

Remove `if is_tui { list[0].clone() }` and always prompt the user to choose.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test disambiguation_test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/cli.rs src/main.rs tests/disambiguation_test.rs
git commit -m "feat(cli): prompt user to disambiguate multiple agents in tab for both TUI and CLI"
```

---

### Task 4: CLI Feature Parity with TUI (Usage, Worktree, and Queue Inspection)

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/main.rs`
- Test: `tests/cli_test.rs`

**Interfaces:**
- `format_boxed_header(terminal_name, current_pane, target, usage_summary)`
- CLI interactive loop handles `queue`, `status`, and prompt editing (`Shift+Left`, `edit <id>`).

- [ ] **Step 1: Write test for updated `format_boxed_header`**

Verify `format_boxed_header` includes `Worktree:` and `Usage:` lines when supplied.

- [ ] **Step 2: Update `format_boxed_header` in `src/cli.rs`**

Add worktree and optional usage lines to the boxed header:
```
┌────────────────────────────────────────────────────────────┐
│  Harness Queue (hq) v0.1                                   │
│  Host: Orca IDE · Location: term_d3b8c331                  │
│  Target: Google Antigravity (term_bbda9ebd)                │
│  Worktree: /Users/romey/Developer/harness-queue            │
│  Usage: Antigravity: Active · Quota available              │
└────────────────────────────────────────────────────────────┘
```

- [ ] **Step 3: Update `src/main.rs` to pass usage summary and support `status` / `queue` in CLI**

Pass `supervisor.get_target_usage_summary()` to header rendering and watch logs. Add `queue` command support in CLI raw loop.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test cli_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs src/main.rs tests/cli_test.rs
git commit -m "feat(cli): add worktree, usage display, and queue parity with TUI"
```

---

### Task 5: End-to-End Verification & Git Release

**Files:**
- Cargo.lock / Cargo.toml if needed

- [ ] **Step 1: Run format check**

Run: `cargo fmt --all -- --check`

- [ ] **Step 2: Run clippy check**

Run: `cargo clippy --all-targets -- -D warnings`

- [ ] **Step 3: Run all unit and integration tests**

Run: `cargo test --all-targets`

- [ ] **Step 4: Build release binary**

Run: `cargo build --release`

- [ ] **Step 5: Push to GitHub**

```bash
git push origin main
```
