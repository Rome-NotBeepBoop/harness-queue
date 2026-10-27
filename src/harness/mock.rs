use super::{
    AgentReadiness, HarnessAdapter, HarnessCapabilities, HarnessError, QuotaStatus, SupportLevel,
};
use crate::models::{AgentTarget, HarnessKind};
use crate::terminal::{AgentStatus, DeliveryAck, TerminalAdapter};
use std::sync::Mutex;

pub struct MockHarness {
    pub kind: HarnessKind,
    pub quota: Mutex<QuotaStatus>,
    pub readiness: Mutex<Option<AgentReadiness>>,
}

impl MockHarness {
    pub fn new(kind: HarnessKind) -> Self {
        Self {
            kind,
            quota: Mutex::new(QuotaStatus::Available),
            readiness: Mutex::new(None),
        }
    }

    pub fn set_quota(&self, quota: QuotaStatus) {
        let mut q = self.quota.lock().unwrap();
        *q = quota;
    }

    pub fn set_readiness(&self, r: AgentReadiness) {
        let mut rd = self.readiness.lock().unwrap();
        *rd = Some(r);
    }
}

impl HarnessAdapter for MockHarness {
    fn kind(&self) -> HarnessKind {
        self.kind.clone()
    }

    fn check_quota(&self, _session_id: &str) -> Result<QuotaStatus, HarnessError> {
        Ok(self.quota.lock().unwrap().clone())
    }

    fn is_ready(
        &self,
        target: &AgentTarget,
        terminal: &dyn TerminalAdapter,
    ) -> Result<AgentReadiness, HarnessError> {
        if let Some(r) = self.readiness.lock().unwrap().clone() {
            return Ok(r);
        }

        match terminal.get_agent_status(&target.pane_id) {
            Ok(AgentStatus::Ready) => Ok(AgentReadiness::Ready),
            Ok(AgentStatus::Busy) => Ok(AgentReadiness::Busy),
            Ok(AgentStatus::BlockedApproval) => Ok(AgentReadiness::BlockedOnApproval(
                "User approval required".to_string(),
            )),
            Ok(AgentStatus::RateLimited) => {
                Ok(AgentReadiness::Interrupted("Rate limited".to_string()))
            }
            _ => Ok(AgentReadiness::Ready),
        }
    }

    fn submit_prompt(
        &self,
        target: &AgentTarget,
        prompt: &str,
        terminal: &dyn TerminalAdapter,
    ) -> Result<DeliveryAck, HarnessError> {
        terminal
            .send_prompt(target, prompt)
            .map_err(|e| HarnessError::ExecutionFailed(e.to_string()))
    }

    fn capabilities(&self) -> HarnessCapabilities {
        match self.kind {
            HarnessKind::Codex => HarnessCapabilities {
                can_track_session: true,
                can_check_quota: true,
                can_auto_resume: true,
                level: SupportLevel::Full,
            },
            HarnessKind::ClaudeCode | HarnessKind::OpenCode => HarnessCapabilities {
                can_track_session: true,
                can_check_quota: false,
                can_auto_resume: true,
                level: SupportLevel::Partial,
            },
            _ => HarnessCapabilities {
                can_track_session: true,
                can_check_quota: false,
                can_auto_resume: false,
                level: SupportLevel::ObserveOnly,
            },
        }
    }
}
