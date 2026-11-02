pub mod agy;
pub mod claude;
pub mod codex;
pub mod mock;
pub mod omp;
pub mod opencode;
pub mod pi;

use crate::models::{AgentTarget, HarnessKind};
use crate::terminal::{DeliveryAck, TerminalAdapter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportLevel {
    Full,
    Partial,
    ObserveOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessCapabilities {
    pub can_track_session: bool,
    pub can_check_quota: bool,
    pub can_auto_resume: bool,
    pub level: SupportLevel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaStatus {
    Available,
    RateLimited {
        resets_at: Option<u64>,
        reason: String,
    },
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentReadiness {
    Ready,
    Busy,
    BlockedOnApproval(String),
    Interrupted(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HarnessError {
    SessionNotFound,
    QuotaCheckFailed(String),
    ExecutionFailed(String),
    UnsupportedCapability(String),
}

impl std::fmt::Display for HarnessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HarnessError::SessionNotFound => write!(f, "Session not found"),
            HarnessError::QuotaCheckFailed(msg) => write!(f, "Quota check failed: {}", msg),
            HarnessError::ExecutionFailed(msg) => write!(f, "Execution failed: {}", msg),
            HarnessError::UnsupportedCapability(msg) => {
                write!(f, "Unsupported capability: {}", msg)
            }
        }
    }
}

impl std::error::Error for HarnessError {}

pub trait HarnessAdapter: Send + Sync {
    fn kind(&self) -> HarnessKind;
    fn check_quota(&self, session_id: &str) -> Result<QuotaStatus, HarnessError>;
    fn is_ready(
        &self,
        target: &AgentTarget,
        terminal: &dyn TerminalAdapter,
    ) -> Result<AgentReadiness, HarnessError>;
    fn submit_prompt(
        &self,
        target: &AgentTarget,
        prompt: &str,
        terminal: &dyn TerminalAdapter,
    ) -> Result<DeliveryAck, HarnessError>;
    fn capabilities(&self) -> HarnessCapabilities;
    fn get_usage_summary(
        &self,
        _target: &AgentTarget,
        _terminal: &dyn TerminalAdapter,
    ) -> Option<String> {
        None
    }
    fn has_active_remaining_quota(
        &self,
        target: &AgentTarget,
        terminal: &dyn TerminalAdapter,
    ) -> bool {
        if let Ok(crate::terminal::AgentStatus::RateLimited) =
            terminal.get_agent_status(&target.pane_id)
        {
            return false;
        }
        if let Ok(QuotaStatus::RateLimited { .. }) = self.check_quota(&target.session_id) {
            return false;
        }
        if let Some(preview) = terminal.get_agent_preview(&target.pane_id) {
            let prev_lower = preview.to_lowercase();
            if prev_lower.contains("rate limit")
                || prev_lower.contains("usage limit")
                || prev_lower.contains(" 0% left")
                || prev_lower.contains(": 0% left")
                || prev_lower.contains("quota exceeded")
            {
                return false;
            }
        }
        true
    }
}

pub fn create_harness_adapter(kind: &HarnessKind) -> Box<dyn HarnessAdapter> {
    match kind {
        HarnessKind::Codex => Box::new(codex::CodexHarness::new()),
        HarnessKind::ClaudeCode => Box::new(claude::ClaudeHarness),
        HarnessKind::OpenCode => Box::new(opencode::OpenCodeHarness),
        HarnessKind::Omp => Box::new(omp::OmpHarness),
        HarnessKind::Pi => Box::new(pi::PiHarness),
        HarnessKind::Agy => Box::new(agy::AgyHarness),
        HarnessKind::Unknown(_) => Box::new(mock::MockHarness::new(kind.clone())),
    }
}
