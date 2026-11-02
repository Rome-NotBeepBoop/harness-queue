# Quota-Exhaustion Hold for Queued Prompts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure `hq` never prematurely sends prompts to an agent while active 5-hour quota remains, holding the queue in `⏳ HELD (Waiting for 0% quota)` until the agent hits 0% quota (rate-limited) and recovers.

**Architecture:** Extend `State` and `ItemStatus` with `Held`. Introduce `has_active_remaining_quota` on `HarnessAdapter` (implemented in `CodexHarness`, `ClaudeHarness`, `MockHarness`), check it during `Supervisor::tick()`, and hold pending queue items. Transition to rate-limited recovery when quota hits 0%, and dispatch once recovered. Update TUI and CLI status badges.

**Tech Stack:** Rust, Ratatui, Crossterm, Serde.

**Spec:** Conversation requirements:
1. Automatically detect when an agent still has active quota remaining.
2. Hold pending prompts instead of immediately sending them into the terminal.
3. Session status in overview must show `⏳ HELD (Waiting for 0% quota)`.
4. Queued items in prompt queue show `[⏳ HELD]`.
5. Dispatch only after remaining quota hits 0% and recovers.

## Global Constraints

- Do not hardcode a percentage like 20%; hold whenever active quota remains in the window before 0%.
- No external notification systems; status must be reflected cleanly in session overview and event logs.
- No override "dispatch now" prompt/buttons.
- Keep all unit tests and CI clippy/formatting checks passing.

## Review Focus

- Codex with active window (`usedPercent < 100.0` and `resets_at.is_some()`): queue holds in `State::Held`.
- Codex reaching 0% quota (`usedPercent >= 100.0` or `rateLimitReachedType`): transitions from `State::Held` to `State::RateLimited` and `State::WaitingForRecovery`.
- Recovery from 0% quota: dispatches pending prompts once reset occurs.
- Agents with no quota window or empty queue: function normally without holding.
- TUI and CLI display `⏳ HELD (Waiting for 0% quota)` and `[⏳ HELD]`.

---

### Task 1: Update State Machine and Queue Models for `Held`

**Files:**
- Modify: `src/models.rs:20-60`
- Modify: `src/state_machine.rs:40-110`
- Modify: `src/queue.rs:60-120`
- Test: `tests/test_state_machine.rs`
- Test: `tests/test_queue.rs`

- [ ] Add `State::Held` to `src/models.rs` and `ItemStatus::Held` to `ItemStatus`.
- [ ] Add transitions to/from `State::Held` in `src/state_machine.rs`:
  - `State::Watching -> State::Held`
  - `State::AgentReady -> State::Held`
  - `State::Held -> State::RateLimited | State::WaitingForRecovery | State::AgentReady | State::AgentBusy | State::Watching`
- [ ] Update `can_dispatch()` in `src/state_machine.rs` so `State::Held` returns `false`.
- [ ] Add helper `mark_held(id: usize)` and `mark_all_pending_held()` in `src/queue.rs`.
- [ ] Add unit tests verifying `State::Held` transitions and queue status handling.
- [ ] Run `cargo test --test test_state_machine --test test_queue` and ensure passing.

---

### Task 2: Implement Active Quota Detection in Harness Adapters

**Files:**
- Modify: `src/harness/mod.rs:25-70`
- Modify: `src/harness/codex.rs:80-130,260-320`
- Modify: `src/harness/claude.rs:40-80`
- Modify: `src/harness/mock.rs:30-80`
- Test: `tests/test_codex_harness.rs`

- [ ] Add `fn has_active_remaining_quota(&self, target: &AgentTarget, terminal: &dyn TerminalAdapter) -> bool` to `HarnessAdapter` trait with default `false`.
- [ ] Implement `has_active_remaining_quota` in `CodexHarness`:
  - Returns `true` if `get_cached_or_fresh_details()` reports `QuotaStatus::Available` and (`resets_at.is_some()` or `used_percent > 0.0`).
  - Returns `false` if `used_percent >= 100.0` or rate limit reached.
- [ ] Implement `has_active_remaining_quota` in `ClaudeHarness`:
  - Inspects terminal preview: if preview indicates token usage and does not indicate rate limit, returns `true`.
- [ ] Implement `has_active_remaining_quota` in `MockHarness` with controllable toggle `set_active_remaining_quota(bool)`.
- [ ] Add unit tests in `tests/test_codex_harness.rs` verifying active quota detection returns true when quota remains and false when exhausted.
- [ ] Run `cargo test --test test_codex_harness` and ensure passing.

---

### Task 3: Integrate Quota Hold in Supervisor Lifecycle

**Files:**
- Modify: `src/supervisor.rs:130-220`
- Test: `tests/test_supervisor.rs`
- Test: `tests/test_all_scenarios.rs`

- [ ] In `Supervisor::tick()`:
  - If `self.queue.has_pending()` and `self.harness.has_active_remaining_quota(&target, self.terminal.as_ref())`:
    - Transition state to `State::Held`.
    - Mark pending queue items as `ItemStatus::Held`.
    - Log once: `events.push("◷ Agent has remaining quota. Queue is HELD until usage reaches 0%".to_string())`.
    - Return `Ok(events)` without dispatching.
  - When quota hits 0%, `check_quota` reports `QuotaStatus::RateLimited`:
    - Transition from `State::Held` to `State::RateLimited` and `State::WaitingForRecovery`.
    - On recovery, transition to `State::RecoveringSession` -> `State::AgentReady` and dispatch!
- [ ] Add unit test in `tests/test_supervisor.rs` demonstrating the complete flow:
  1. Queue prompt with active quota -> enters `State::Held`, items marked `Held`, no dispatch.
  2. Agent quota hits 0% -> enters `State::RateLimited` / `WaitingForRecovery`.
  3. Reset timestamp passes -> enters `RecoveringSession` / `AgentReady` and dispatches prompt!
- [ ] Run `cargo test --test test_supervisor` and ensure passing.

---

### Task 4: Update TUI and CLI UI for `⏳ HELD` Status

**Files:**
- Modify: `src/tui/app.rs:500-680`
- Modify: `src/cli.rs:20-80`
- Test: `tests/tui_test.rs`
- Test: `tests/test_cli_ux.rs`

- [ ] In `src/tui/app.rs`:
  - Update `state_badge` for `State::Held`:
    ```rust
    State::Held => (
        Span::styled(
            " ⏳ HELD (Waiting for 0% quota) ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        32,
    ),
    ```
  - Update queue item status badge for `ItemStatus::Held`:
    ```rust
    ItemStatus::Held => (
        "[⏳ HELD]",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ),
    ```
  - Allow Shift+Left / mouse click editing on `ItemStatus::Held` items.
- [ ] Add tests in `tests/tui_test.rs` verifying `State::Held` renders `⏳ HELD (Waiting for 0% quota)`.
- [ ] Run `cargo test --test tui_test` and ensure passing.

---

### Task 5: Full Verification, Binary Compilation, and Local Installation

**Files:**
- All modified files

- [ ] Run `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings`.
- [ ] Run `cargo test --all-targets` (all 37+ tests must pass).
- [ ] Compile release binary: `cargo build --release`.
- [ ] Install and re-sign binary to `~/.local/bin/hq` and `~/.cargo/bin/hq`:
  `cp -f target/release/hq ~/.local/bin/hq && codesign --force -s - ~/.local/bin/hq`.
- [ ] Keep commits local (do NOT push to GitHub main without explicit user authorization).
