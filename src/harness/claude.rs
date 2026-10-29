use super::{
    AgentReadiness, HarnessAdapter, HarnessCapabilities, HarnessError, QuotaStatus, SupportLevel,
};
use crate::models::{AgentTarget, HarnessKind};
use crate::terminal::{AgentStatus, DeliveryAck, TerminalAdapter};

pub struct ClaudeHarness;

impl HarnessAdapter for ClaudeHarness {
    fn kind(&self) -> HarnessKind {
        HarnessKind::ClaudeCode
    }

    fn check_quota(&self, _session_id: &str) -> Result<QuotaStatus, HarnessError> {
        // Claude Code has no public local JSON-RPC quota endpoint, report Available by default
        Ok(QuotaStatus::Available)
    }

    fn is_ready(
        &self,
        target: &AgentTarget,
        terminal: &dyn TerminalAdapter,
    ) -> Result<AgentReadiness, HarnessError> {
        match terminal.get_agent_status(&target.pane_id) {
            Ok(AgentStatus::Ready) => Ok(AgentReadiness::Ready),
            Ok(AgentStatus::Busy) => Ok(AgentReadiness::Busy),
            Ok(AgentStatus::BlockedApproval) => Ok(AgentReadiness::BlockedOnApproval(
                "Claude permission dialog pending".to_string(),
            )),
            Ok(AgentStatus::RateLimited) => Ok(AgentReadiness::Interrupted(
                "Claude rate limit observed".to_string(),
            )),
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

    fn get_usage_summary(
        &self,
        target: &AgentTarget,
        terminal: &dyn TerminalAdapter,
    ) -> Option<String> {
        let status = terminal.get_agent_status(&target.pane_id).ok();
        let preview = terminal.get_agent_preview(&target.pane_id);

        if let Some(AgentStatus::RateLimited) = status {
            return Some("Claude Code: Rate limit observed · Waiting for recovery".to_string());
        }

        if let Some(prev) = preview {
            let prev_lower = prev.to_lowercase();
            if prev_lower.contains("rate limit") {
                return Some("Claude Code: Rate limit observed · Waiting".to_string());
            }
            if let Some(pct_idx) = prev.find('%') {
                let start = prev[..pct_idx].rfind(' ').map(|i| i + 1).unwrap_or(0);
                let end = prev[pct_idx..].find(' ').map(|i| pct_idx + i).unwrap_or(prev.len());
                let snippet = prev[start..end].trim();
                if snippet.contains('%') {
                    return Some(format!("Claude Code: {} tokens · Ready", snippet));
                }
            }
        }

        match status {
            Some(AgentStatus::Busy) => Some("Claude Code: Busy · Active turn".to_string()),
            Some(AgentStatus::BlockedApproval) => {
                Some("Claude Code: Blocked on user approval".to_string())
            }
            _ => Some("Claude Code: Active · Ready (no limit observed)".to_string()),
        }
    }
}
