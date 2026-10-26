use super::{
    AgentReadiness, HarnessAdapter, HarnessCapabilities, HarnessError, QuotaStatus, SupportLevel,
};
use crate::models::{AgentTarget, HarnessKind};
use crate::terminal::{AgentStatus, DeliveryAck, TerminalAdapter};

pub struct OpenCodeHarness;

impl HarnessAdapter for OpenCodeHarness {
    fn kind(&self) -> HarnessKind {
        HarnessKind::OpenCode
    }

    fn check_quota(&self, _session_id: &str) -> Result<QuotaStatus, HarnessError> {
        Ok(QuotaStatus::Available)
    }

    fn is_ready(&self, target: &AgentTarget, terminal: &dyn TerminalAdapter) -> Result<AgentReadiness, HarnessError> {
        match terminal.get_agent_status(&target.pane_id) {
            Ok(AgentStatus::Ready) => Ok(AgentReadiness::Ready),
            Ok(AgentStatus::Busy) => Ok(AgentReadiness::Busy),
            Ok(AgentStatus::BlockedApproval) => {
                Ok(AgentReadiness::BlockedOnApproval("OpenCode permission prompt".to_string()))
            }
            Ok(AgentStatus::RateLimited) => Ok(AgentReadiness::Interrupted("OpenCode rate limit".to_string())),
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
        HarnessCapabilities {
            can_track_session: true,
            can_check_quota: false,
            can_auto_resume: true,
            level: SupportLevel::Partial,
        }
    }
}
