use crate::harness::{AgentReadiness, HarnessAdapter, QuotaStatus};
use crate::models::{AgentTarget, Mode, State, DEFAULT_CONTINUATION_PROMPT};
use crate::queue::QueueEngine;
use crate::state_machine::StateMachine;
use crate::terminal::{NeighborDetection, TerminalAdapter};
use std::sync::Arc;

pub struct Supervisor {
    pub terminal: Arc<dyn TerminalAdapter>,
    pub harness: Arc<dyn HarnessAdapter>,
    pub mode: Mode,
    pub target: Option<AgentTarget>,
    pub queue: QueueEngine,
    pub state_machine: StateMachine,
    pub quota_resets_at: Option<u64>,
    pub last_sent_id: Option<usize>,
    pub reported_waiting_busy: bool,
    pub reported_waiting_quota: bool,
}

impl Supervisor {
    pub fn new(
        terminal: Arc<dyn TerminalAdapter>,
        harness: Arc<dyn HarnessAdapter>,
        mode: Mode,
    ) -> Self {
        Self {
            terminal,
            harness,
            mode,
            target: None,
            queue: QueueEngine::new(),
            state_machine: StateMachine::new(),
            quota_resets_at: None,
            last_sent_id: None,
            reported_waiting_busy: false,
            reported_waiting_quota: false,
        }
    }

    pub fn initialize(&mut self) -> Result<AgentTarget, String> {
        let current_pane = self
            .terminal
            .detect_current_pane()
            .map_err(|e| format!("Could not detect current pane: {}", e))?;

        let detection = self
            .terminal
            .find_adjacent_agent(&current_pane)
            .map_err(|e| format!("Error finding adjacent agent: {}", e))?;

        let target = match detection {
            NeighborDetection::Exact(t) => t,
            NeighborDetection::Ambiguous(list) => {
                return Err(format!(
                    "Multiple adjacent candidate agents found ({}). Ambiguous pairing.",
                    list.len()
                ));
            }
            NeighborDetection::NoneFound(msg) => {
                return Err(format!("No adjacent agent found: {}", msg));
            }
        };

        self.target = Some(target.clone());
        let _ = self.state_machine.transition(State::SelectingMode);
        let _ = self.state_machine.transition(State::Watching);

        if self.mode == Mode::AutoResume {
            self.enqueue_prompt(DEFAULT_CONTINUATION_PROMPT.to_string());
        }

        Ok(target)
    }

    pub fn enqueue_prompt(&mut self, prompt: String) -> usize {
        self.queue.enqueue(prompt)
    }

    pub fn enqueue_prompt_with_attachments(
        &mut self,
        prompt: String,
        attachments_summary: Option<String>,
    ) -> usize {
        self.queue
            .enqueue_with_attachments(prompt, attachments_summary)
    }

    pub fn update_queued_prompt(
        &mut self,
        id: usize,
        new_prompt: String,
        new_attachments: Option<String>,
    ) -> Result<(), String> {
        self.queue
            .update_prompt(id, new_prompt, new_attachments)
            .map_err(|e| e.to_string())
    }

    pub fn tick(&mut self) -> Result<Vec<String>, String> {
        let target = match &self.target {
            Some(t) => t.clone(),
            None => return Err("Supervisor not initialized with target".to_string()),
        };

        let mut events = Vec::new();

        // 1. Verify target process integrity (PID / pane check across all terminals)
        let alive = self
            .terminal
            .verify_target_alive(&target)
            .map_err(|e| format!("Target verification error: {}", e))?;

        if !alive {
            let _ = self.state_machine.transition(State::Error);
            return Err(
                "Target agent process changed or died! Halting prompt delivery safely.".to_string(),
            );
        }

        // Halt dispatch if any item is in uncertain delivery
        if self
            .queue
            .all_items()
            .iter()
            .any(|i| i.status == crate::models::ItemStatus::Uncertain)
        {
            return Ok(events);
        }

        // 2. Check quota status
        let quota = self
            .harness
            .check_quota(&target.session_id)
            .map_err(|e| format!("Quota check failed: {}", e))?;

        match quota {
            QuotaStatus::RateLimited {
                resets_at,
                reason: _,
            } => {
                self.quota_resets_at = resets_at;
                if !self.reported_waiting_quota {
                    let _ = self.state_machine.transition(State::RateLimited);
                    let _ = self.state_machine.transition(State::WaitingForRecovery);
                    if let Some(ts) = resets_at {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        if ts > now {
                            let diff = ts - now;
                            let hours = diff / 3600;
                            let mins = (diff % 3600) / 60;
                            if hours > 0 {
                                events.push(format!(
                                    "◷ Waiting for quota recovery (resets in {}h {}m)",
                                    hours, mins
                                ));
                            } else {
                                events.push(format!(
                                    "◷ Waiting for quota recovery (resets in {}m)",
                                    mins
                                ));
                            }
                        } else {
                            events.push("◷ Waiting for quota recovery (reset due)".to_string());
                        }
                    } else {
                        events.push("◷ Waiting for quota recovery".to_string());
                    }
                    self.reported_waiting_quota = true;
                }
                return Ok(events);
            }
            QuotaStatus::Available => {
                if self.reported_waiting_quota {
                    events.push("✓ Usage reset detected".to_string());
                    events.push("✓ Original session recovered".to_string());
                    self.reported_waiting_quota = false;
                    let _ = self.state_machine.transition(State::RecoveringSession);
                }
            }
            QuotaStatus::Unknown => {}
        }

        // 3. Check agent readiness
        let readiness = self
            .harness
            .is_ready(&target, self.terminal.as_ref())
            .map_err(|e| format!("Readiness check failed: {}", e))?;

        match readiness {
            AgentReadiness::Busy => {
                let _ = self.state_machine.transition(State::AgentBusy);
                if !self.reported_waiting_busy {
                    if let Some(last_id) = self.last_sent_id {
                        events.push(format!(
                            "◷ Waiting for {} to finish prompt #{}...",
                            target.harness.display_name(),
                            last_id
                        ));
                    } else {
                        events.push(format!(
                            "◷ Waiting for {} to finish active turn...",
                            target.harness.display_name()
                        ));
                    }
                    self.reported_waiting_busy = true;
                }
                return Ok(events);
            }
            AgentReadiness::BlockedOnApproval(reason) => {
                // Agent requires approval, do NOT dispatch and do NOT treat as rate limit
                if !self.reported_waiting_busy {
                    events.push(format!("! Agent blocked on approval: {}", reason));
                    self.reported_waiting_busy = true;
                }
                return Ok(events);
            }
            AgentReadiness::Interrupted(reason) => {
                events.push(format!("! Agent turn interrupted: {}", reason));
                return Ok(events);
            }
            AgentReadiness::Ready => {
                if self.reported_waiting_busy {
                    if let Some(last_id) = self.last_sent_id {
                        events.push(format!(
                            "✓ {} finished prompt #{}",
                            target.harness.display_name(),
                            last_id
                        ));
                    }
                    self.reported_waiting_busy = false;
                }
                let _ = self.state_machine.transition(State::AgentReady);
            }
        }

        // 4. Dispatch next pending queue item if ready
        if self.state_machine.can_dispatch() {
            if let Some(next_item) = self.queue.peek_next_pending().cloned() {
                let id = next_item.id;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                let _ = self.state_machine.transition(State::Submitting);
                let _ = self.queue.mark_submitting(id, now);

                if self.mode == Mode::AutoResume && id == 1 {
                    events.push("➤ Sending continuation prompt...".to_string());
                } else {
                    events.push(format!("➤ Sending prompt #{}...", id));
                }

                // Submit prompt
                let ack_res =
                    self.harness
                        .submit_prompt(&target, &next_item.prompt, self.terminal.as_ref());

                match ack_res {
                    Ok(ack) if ack.acknowledged => {
                        let _ = self.queue.mark_acknowledged(id);
                        let _ = self.state_machine.transition(State::PromptAcknowledged);
                        self.last_sent_id = Some(id);
                        events.push(format!("✓ Prompt #{} sent!", id));
                    }
                    Ok(_) | Err(_) => {
                        let _ = self.queue.mark_uncertain(id);
                        let _ = self.state_machine.transition(State::DeliveryUncertain);
                        events.push(format!(
                            "! Prompt #{} delivery uncertain; paused for inspection",
                            id
                        ));
                    }
                }
            }
        }

        Ok(events)
    }
}
