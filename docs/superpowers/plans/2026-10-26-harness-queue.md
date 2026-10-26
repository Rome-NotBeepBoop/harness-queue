# Harness Queue (`hq`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `hq` (Harness Queue), a minimal, universal, terminal-based agent queue in Rust that runs alongside existing coding agents in herdr or Ghostty panes, detects adjacent agents, provides Auto-resume and Auto-send modes, and maintains a continuous FIFO prompt queue across quota interruptions and agent idle states.

**Architecture:** A standalone, native Rust binary with decoupled terminal adapters (Herdr, Ghostty, and Mock), harness adapters (Codex with full quota/app-server tracking, Claude Code, OpenCode, OMP, Pi, AGY), an explicit state machine for queue lifecycle and quota recovery, and an interactive CLI input loop that accepts continuous prompts without blocking on active turns.

**Tech Stack:** Rust (cargo, rustc 1.98), standard library, `serde` / `serde_json` for JSON serialization, `chrono` for timestamps, and native macOS CLI / AppleScript / Unix socket integrations.

**Spec:** USER_REQUEST specification in prompt context.

## Global Constraints

- One standalone `hq` executable with native macOS support first.
- Zero special wrappers: HQ must discover and supervise an already running agent in an adjacent pane.
- Two mutually exclusive modes: Mode 1 (Auto-resume with exact predefined prompt) and Mode 2 (Auto-send for custom prompts).
- Continuous queue behavior: HQ must not exit after submitting the first prompt; it immediately prompts for the next input.
- Pinned target identity: Once paired, target identity is locked; layout or process replacements halt rather than silently redirect prompts.
- Distinguish approval prompts (`blocked`), network failures, and authentications from genuine quota limits (`rate_limited`).
- Do not blindly retry ambiguous submissions; distinguish queued, submitting, acknowledged, and uncertain states.
- Clean detachment on Ctrl+C without stopping the agent.

## Review Focus

- Pane replacement during run: Ensure HQ verifies target PID/pane before every prompt dispatch and halts safely if the process changed.
- Approval vs Quota disambiguation: Ensure Herdr `blocked` state or interactive permission requests are never classified as quota exhaustion.
- Quota reset clock vs actual recovery: Ensure reset timestamps reaching zero do not dispatch until quota availability is verified from provider evidence.
- Multiline prompt entry: Ensure the CLI accepts user prompts without breaking the non-TUI terminal state or dropping queued entries.
- Ghostty permission denial: Ensure missing macOS AppleScript / Automation permission produces a clean, actionable error instead of a crash.

---

### Task 1: Project Scaffolding and Core Domain Models

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/lib.rs`
- Create: `src/models.rs`
- Test: `tests/test_models.rs`

**Interfaces:**
- Consumes: Standard Rust types, Serde
- Produces:
  - `enum Mode { AutoResume, AutoSend }`
  - `enum State { Detecting, SelectingMode, Watching, RateLimited, WaitingForRecovery, RecoveringSession, AgentBusy, AgentReady, Submitting, PromptAcknowledged, DeliveryUncertain, Error, Detached }`
  - `enum ItemStatus { Queued, Submitting, Acknowledged, Uncertain }`
  - `struct QueueItem { id: usize, prompt: String, status: ItemStatus, queued_at: u64, submitted_at: Option<u64> }`
  - `struct AgentTarget { terminal_id: String, pane_id: String, pid: u32, harness: HarnessKind, session_id: String, cwd: String }`
  - `enum HarnessKind { Codex, ClaudeCode, OpenCode, Omp, Pi, Agy, Unknown(String) }`
  - `const DEFAULT_CONTINUATION_PROMPT: &str = "Continue the unfinished work. Inspect the session history and repository state before making changes. Do not repeat completed tasks.";`

- [ ] **Step 1: Write the failing test for domain models**

Write `tests/test_models.rs`:
```rust
use harness_queue::models::{Mode, State, ItemStatus, QueueItem, HarnessKind, AgentTarget, DEFAULT_CONTINUATION_PROMPT};

#[test]
fn test_default_continuation_prompt_exact_text() {
    assert_eq!(
        DEFAULT_CONTINUATION_PROMPT,
        "Continue the unfinished work. Inspect the session history and repository state before making changes. Do not repeat completed tasks."
    );
}

#[test]
fn test_queue_item_lifecycle() {
    let mut item = QueueItem::new(1, "Fix tests".to_string());
    assert_eq!(item.status, ItemStatus::Queued);
    item.mark_submitting();
    assert_eq!(item.status, ItemStatus::Submitting);
    item.mark_acknowledged();
    assert_eq!(item.status, ItemStatus::Acknowledged);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_models`
Expected: FAIL (types / crate not yet found)

- [ ] **Step 3: Implement `Cargo.toml`, `src/lib.rs`, `src/models.rs`, `src/main.rs`**

Set up `Cargo.toml` with `serde`, `serde_json`, and implement domain models in `src/models.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_models`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/ tests/
git commit -m "feat: scaffolding and core domain models"
```

---

### Task 2: State Machine and FIFO Queue Engine

**Files:**
- Create: `src/queue.rs`
- Create: `src/state_machine.rs`
- Modify: `src/lib.rs`
- Test: `tests/test_queue.rs`
- Test: `tests/test_state_machine.rs`

**Interfaces:**
- Consumes: `models::{Mode, State, ItemStatus, QueueItem, AgentTarget}`
- Produces:
  - `struct QueueEngine`: manages FIFO queue items, next sequence, dispatch eligibility, deduplication.
  - `struct StateMachine`: tracks transition validity, handles rate-limit and recovery events, prevents invalid submissions.
  - `fn can_dispatch(&self) -> bool`
  - `fn transition(&mut self, next: State) -> Result<(), StateError>`

- [ ] **Step 1: Write the failing tests for FIFO queue and state machine**

Write `tests/test_queue.rs` (testing FIFO order, queue additions while busy, no duplicate delivery) and `tests/test_state_machine.rs` (testing valid state transitions and blocking submissions when rate-limited or busy).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_queue --test test_state_machine`
Expected: FAIL

- [ ] **Step 3: Implement `src/queue.rs` and `src/state_machine.rs`**

Implement strict state transitions:
- Disallow dispatch when state is `RateLimited`, `WaitingForRecovery`, `AgentBusy`, or `Submitting`.
- Allow dispatch only in `AgentReady`.
- Support transitions: `Watching` -> `RateLimited` -> `WaitingForRecovery` -> `RecoveringSession` -> `AgentReady` -> `Submitting` -> `PromptAcknowledged` -> `AgentBusy` -> `AgentReady`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_queue --test test_state_machine`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/queue.rs src/state_machine.rs src/lib.rs tests/
git commit -m "feat: queue engine and state machine"
```

---

### Task 3: Terminal Adapter Abstraction and Herdr Adapter

**Files:**
- Create: `src/terminal/mod.rs`
- Create: `src/terminal/herdr.rs`
- Create: `src/terminal/mock.rs`
- Modify: `src/lib.rs`
- Test: `tests/test_herdr_adapter.rs`

**Interfaces:**
- Consumes: `models::*`
- Produces:
  - `trait TerminalAdapter`:
    - `fn detect(&self) -> Result<Option<TerminalContext>, TerminalError>`
    - `fn find_adjacent_agent(&self, current: &TerminalContext) -> Result<AdjacentResult, TerminalError>`
    - `fn get_agent_status(&self, pane_id: &str) -> Result<AgentLifecycle, TerminalError>`
    - `fn send_prompt(&self, target: &AgentTarget, prompt: &str) -> Result<DeliveryAck, TerminalError>`
    - `fn verify_target_process(&self, target: &AgentTarget) -> Result<bool, TerminalError>`
  - `struct HerdrAdapter`: executes `herdr` CLI commands (`pane neighbor`, `pane layout`, `pane process-info`, `agent get`, `agent prompt`) and parses JSON.
  - `struct MockTerminalAdapter`: in-memory controllable adapter for unit and integration testing.

- [ ] **Step 1: Write the failing test for Herdr adapter**

Write `tests/test_herdr_adapter.rs`:
- Test parsing of `herdr pane neighbor` and `herdr pane layout` responses.
- Test single neighbor detection vs ambiguous neighbors.
- Test process replacement detection (PID change).
- Test prompt submission and acknowledgement.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_herdr_adapter`
Expected: FAIL

- [ ] **Step 3: Implement `src/terminal/mod.rs`, `src/terminal/herdr.rs`, and `src/terminal/mock.rs`**

Implement `HerdrAdapter`:
- Check `HERDR_ENV == "1"` and `herdr status`.
- Read caller pane from `HERDR_PANE_ID`.
- Query neighbor using `herdr pane neighbor --direction right` (and left/up/down fallback).
- Inspect `herdr pane process-info` and `herdr agent get`.
- Pin target pane and PID.
- Submit via `herdr agent prompt <target> <text>` with acknowledgment validation.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_herdr_adapter`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/terminal/ tests/test_herdr_adapter.rs
git commit -m "feat: terminal adapter trait, herdr adapter, and mock adapter"
```

---

### Task 4: Harness Adapter Abstraction and Codex Implementation

**Files:**
- Create: `src/harness/mod.rs`
- Create: `src/harness/codex.rs`
- Create: `src/harness/mock.rs`
- Modify: `src/lib.rs`
- Test: `tests/test_codex_harness.rs`

**Interfaces:**
- Consumes: `models::*`, `terminal::TerminalAdapter`
- Produces:
  - `trait HarnessAdapter`:
    - `fn harness_kind(&self) -> HarnessKind`
    - `fn identify_session(&self, pane_info: &PaneProcessInfo) -> Result<HarnessSession, HarnessError>`
    - `fn check_quota(&self, session: &HarnessSession) -> Result<QuotaStatus, QuotaError>`
    - `fn is_ready(&self, session: &HarnessSession, terminal: &dyn TerminalAdapter) -> Result<AgentReadiness, HarnessError>`
    - `fn resume_session(&self, session: &HarnessSession, terminal: &dyn TerminalAdapter) -> Result<(), HarnessError>`
  - `struct CodexHarness`:
    - Interacts with `codex app-server --listen stdio://` for structured `account/rateLimits/read` and `thread/list`.
    - Distinguishes quota exhaustion (`rateLimitReachedType != null` or `usedPercent >= 100`) from normal idle/approval.
    - Accurately tracks reset timestamps (`resetsAt`).
  - `struct MockHarness`: controllable harness for simulation tests.

- [ ] **Step 1: Write the failing test for Codex harness**

Write `tests/test_codex_harness.rs`:
- Test parsing of `account/rateLimits/read` snapshots.
- Test detection of genuine rate-limit vs quota available.
- Test session discovery by cwd and session ID.
- Test that approval state (`blocked`) is never classified as rate-limited.
- Test reset timestamp handling (not dispatching until verified available).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_codex_harness`
Expected: FAIL

- [ ] **Step 3: Implement `src/harness/mod.rs`, `src/harness/codex.rs`, and `src/harness/mock.rs`**

Implement `CodexHarness` with stdio JSON-RPC communication to `codex app-server` or CLI fallback, parsing rate limit windows and session status.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_codex_harness`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/harness/ tests/test_codex_harness.rs
git commit -m "feat: harness adapter abstraction and codex harness implementation"
```

---

### Task 5: Ghostty Terminal Adapter (macOS AppleScript)

**Files:**
- Create: `src/terminal/ghostty.rs`
- Modify: `src/terminal/mod.rs`
- Test: `tests/test_ghostty_adapter.rs`

**Interfaces:**
- Consumes: `terminal::TerminalAdapter`, `models::*`
- Produces:
  - `struct GhosttyAdapter`:
    - Uses `osascript` to query Ghostty 1.3.0+ scripting dictionary.
    - Discovers terminals in `selected tab of front window`.
    - Discovers caller terminal via TTY or cwd, and identifies neighbor terminal.
    - Submits prompts via AppleScript `input text` and `send key "enter"`.
    - Detects macOS Automation permission denial (-1743) and produces actionable guidance.

- [ ] **Step 1: Write the failing test for Ghostty adapter**

Write `tests/test_ghostty_adapter.rs`:
- Test AppleScript script generation and response parsing.
- Test identification of neighbor terminal from tab terminals.
- Test permission error handling (parsing -1743).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_ghostty_adapter`
Expected: FAIL

- [ ] **Step 3: Implement `src/terminal/ghostty.rs`**

Implement `GhosttyAdapter` with safe `osascript` execution, timeout handling, and automation permission detection.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_ghostty_adapter`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/terminal/ghostty.rs src/terminal/mod.rs tests/test_ghostty_adapter.rs
git commit -m "feat: ghostty terminal adapter with applescript integration"
```

---

### Task 6: Additional Harness Adapters (Claude Code, OpenCode, OMP, Pi, AGY)

**Files:**
- Create: `src/harness/claude.rs`
- Create: `src/harness/opencode.rs`
- Create: `src/harness/omp.rs`
- Create: `src/harness/pi.rs`
- Create: `src/harness/agy.rs`
- Modify: `src/harness/mod.rs`
- Test: `tests/test_additional_harnesses.rs`

**Interfaces:**
- Consumes: `harness::HarnessAdapter`, `models::*`
- Produces:
  - Adapter implementations for:
    - Claude Code (`claude`): session files in `~/.claude/sessions` or project config, terminal hook detection.
    - OpenCode (`opencode`): session inspection via `opencode api` or session DB.
    - OMP (`omp`): session directory and profile detection.
    - Pi (`pi`): session storage in `~/.pi/sessions` or project `.pi`.
    - AGY (`agy`): conversation detection via CLI transcript directory.
  - Progressive compatibility classification (Full, Partial, Observe-Only).

- [ ] **Step 1: Write the failing test for additional harnesses**

Write `tests/test_additional_harnesses.rs`:
- Test harness kind detection from process command line (`claude`, `opencode`, `omp`, `pi`, `agy`).
- Test session identification and capability reporting.
- Test truthful capability limitation reporting.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_additional_harnesses`
Expected: FAIL

- [ ] **Step 3: Implement the harness adapters in `src/harness/`**

Implement detection and session resolution for each harness according to their installed CLIs and configuration paths.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_additional_harnesses`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/harness/ tests/test_additional_harnesses.rs
git commit -m "feat: progressive harness adapters for claude, opencode, omp, pi, agy"
```

---

### Task 7: Continuous Queue Supervisor and Prompt Input CLI

**Files:**
- Create: `src/supervisor.rs`
- Create: `src/cli.rs`
- Modify: `src/main.rs`
- Test: `tests/test_supervisor.rs`
- Test: `tests/test_cli_ux.rs`

**Interfaces:**
- Consumes: `models::*`, `state_machine::*`, `queue::*`, `terminal::*`, `harness::*`
- Produces:
  - `struct Supervisor`: runs continuous monitoring loop, handles quota pauses, dispatches queued prompts, tracks acknowledgements.
  - `struct CliUi`: manages interactive stdin/stdout, mode selection (1 or 2), continuous prompt entry (`› Next prompt:`), clean detachment on Ctrl+C.
  - Exact UX matching spec:
    - `✓ Herdr detected` / `✓ Adjacent agent: ...`
    - Selection prompt: `› Select [1]:`
    - Prompt queuing: `✓ Prompt #N queued`
    - Submission reporting: `✓ Prompt #N sent!`
    - Immediate follow-up: `› Next prompt:`

- [ ] **Step 1: Write the failing test for supervisor and CLI loop**

Write `tests/test_supervisor.rs`:
- Test that `Prompt sent!` triggers immediate input eligibility for next prompt.
- Test that prompts queue up while agent is working.
- Test that prompt #2 dispatches automatically when prompt #1 completes.
- Test clean shutdown on signal.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_supervisor`
Expected: FAIL

- [ ] **Step 3: Implement `src/supervisor.rs`, `src/cli.rs`, and update `src/main.rs`**

Implement background monitoring and foreground non-blocking prompt entry.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_supervisor`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/supervisor.rs src/cli.rs src/main.rs tests/
git commit -m "feat: continuous queue supervisor and interactive cli loop"
```

---

### Task 8: Complete Automated Test Suite (All 20 Required Scenarios)

**Files:**
- Create: `tests/test_all_scenarios.rs`
- Test: `tests/test_all_scenarios.rs`

**Interfaces:**
- Exercises the full stack using `MockTerminalAdapter` and `MockHarness` alongside real adapter logic.
- Verifies scenarios 1 through 20:
  1. Neighbor detection inside herdr.
  2. Auto-resume exact default prompt.
  3. Auto-send custom prompt.
  4. Prompt stays queued before quota recovery.
  5. Reset timestamp alone does not trigger submission.
  6. Confirmed recovery enables dispatch when session is ready.
  7. `Prompt sent!` reporting after acknowledged submission.
  8. Immediately allows second prompt.
  9. Second prompt waits while agent is busy.
  10. Second prompt dispatches when agent becomes idle.
  11. Additional prompts maintain FIFO ordering.
  12. Rate-limit pause safely holds queue.
  13. Duplicate lifecycle events do not double-submit.
  14. Approval-blocked agent is not mistaken for rate-limited.
  15. Ambiguous neighboring pane never receives automatic input.
  16. Ctrl+C detachment without stopping agent.
  17. Restart does not blindly resend uncertain submissions.
  18. Pane change / process replacement halts submission.
  19. Ghostty missing Automation permissions produces clear error.
  20. Unsupported harness capabilities reported truthfully.

- [ ] **Step 1: Write the failing tests in `tests/test_all_scenarios.rs`**

- [ ] **Step 2: Run tests to verify any failures**

Run: `cargo test --test test_all_scenarios`

- [ ] **Step 3: Resolve any test edge cases and refine implementations**

- [ ] **Step 4: Run all tests to verify 100% pass**

Run: `cargo test`
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add tests/test_all_scenarios.rs src/
git commit -m "test: full 20-scenario verification suite"
```

---

### Task 9: Real Local End-to-End Verification, Documentation, and Support Matrix

**Files:**
- Create: `README.md`
- Create: `docs/architecture.md`
- Create: `docs/support_matrix.md`
- Modify: `Cargo.toml`
- Test: Live CLI invocation check

**Interfaces:**
- Produces:
  - Working compiled binary `target/release/hq`
  - Complete README with usage, installation, and real examples.
  - Architecture and adapter design docs.
  - Accurate support matrix per terminal and harness.

- [ ] **Step 1: Build release binary**

Run: `cargo build --release`
Verify binary exists at `target/release/hq`.

- [ ] **Step 2: Run live verification test in Herdr session with real Codex/harmless prompt**

Verify live neighbor detection, mode selection, and prompt queuing.

- [ ] **Step 3: Write `README.md`, `docs/architecture.md`, and `docs/support_matrix.md`**

Document build/run instructions, verified terminals/harnesses, partial capabilities, and usage examples.

- [ ] **Step 4: Run full verification checklist**

Run: `cargo test` and verify clean build with zero warnings.

- [ ] **Step 5: Commit**

```bash
git add README.md docs/
git commit -m "docs: user guide, architecture, and support matrix"
```
