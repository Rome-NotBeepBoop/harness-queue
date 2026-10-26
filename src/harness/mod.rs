pub mod codex;
pub mod mock;

use crate::models::{AgentTarget, HarnessKind};
use crate::terminal::{DeliveryAck, TerminalAdapter};

#[derive(Debug, Clone, PartialEq, Eq)]
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
            HarnessError::UnsupportedCapability(msg) => write!(f, "Unsupported capability: {}", msg),
        }
    }
}

impl std::error::Error for HarnessError {}

pub trait HarnessAdapter: Send + Sync {
    fn kind(&self) -> HarnessKind;
    fn check_quota(&self, session_id: &str) -> Result<QuotaStatus, HarnessError>;
    fn is_ready(&self, target: &AgentTarget, terminal: &dyn TerminalAdapter) -> Result<AgentReadiness, HarnessError>;
    fn submit_prompt(
        &self,
        target: &AgentTarget,
        prompt: &str,
        terminal: &dyn TerminalAdapter,
    ) -> Result<DeliveryAck, HarnessError>;
    fn capabilities(&self) -> HarnessCapabilities;
}
