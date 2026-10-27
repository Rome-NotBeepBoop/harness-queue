use super::{AgentStatus, DeliveryAck, NeighborDetection, TerminalAdapter, TerminalError};
use crate::models::{AgentTarget, HarnessKind};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

#[derive(Debug)]
pub struct MockTerminalAdapter {
    pub current_pane: String,
    pub target: AgentTarget,
    pub status: Mutex<AgentStatus>,
    pub prompt_count: AtomicU32,
}

impl Default for MockTerminalAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl MockTerminalAdapter {
    pub fn new() -> Self {
        Self {
            current_pane: "w1:p1".to_string(),
            target: AgentTarget {
                terminal_id: "term_mock_1".to_string(),
                pane_id: "w1:p2".to_string(),
                pid: 12345,
                harness: HarnessKind::Codex,
                session_id: "7f2a-mock".to_string(),
                cwd: "/Users/romey/Developer/harness-queue".to_string(),
            },
            status: Mutex::new(AgentStatus::Ready),
            prompt_count: AtomicU32::new(0),
        }
    }

    pub fn set_status(&self, new_status: AgentStatus) {
        let mut s = self.status.lock().unwrap();
        *s = new_status;
    }
}

impl TerminalAdapter for MockTerminalAdapter {
    fn terminal_name(&self) -> &'static str {
        "Mock"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn detect_current_pane(&self) -> Result<String, TerminalError> {
        Ok(self.current_pane.clone())
    }

    fn find_adjacent_agent(&self, _current_pane: &str) -> Result<NeighborDetection, TerminalError> {
        Ok(NeighborDetection::Exact(self.target.clone()))
    }

    fn get_agent_status(&self, pane_id: &str) -> Result<AgentStatus, TerminalError> {
        if pane_id == self.target.pane_id {
            Ok(self.status.lock().unwrap().clone())
        } else {
            Err(TerminalError::NeighborNotFound)
        }
    }

    fn send_prompt(
        &self,
        target: &AgentTarget,
        _prompt: &str,
    ) -> Result<DeliveryAck, TerminalError> {
        if target.pane_id != self.target.pane_id {
            return Err(TerminalError::NeighborNotFound);
        }
        self.prompt_count.fetch_add(1, Ordering::SeqCst);
        self.set_status(AgentStatus::Busy);
        Ok(DeliveryAck {
            acknowledged: true,
            details: "Mock delivery acknowledged".to_string(),
        })
    }

    fn verify_target_alive(&self, target: &AgentTarget) -> Result<bool, TerminalError> {
        Ok(target.pid == self.target.pid && target.pane_id == self.target.pane_id)
    }
}
