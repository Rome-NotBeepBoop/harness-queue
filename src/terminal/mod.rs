pub mod ghostty;
pub mod herdr;
pub mod mock;

use crate::models::AgentTarget;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentStatus {
    Ready,
    Busy,
    BlockedApproval,
    RateLimited,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NeighborDetection {
    Exact(AgentTarget),
    Ambiguous(Vec<AgentTarget>),
    NoneFound(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryAck {
    pub acknowledged: bool,
    pub details: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalError {
    NotDetected,
    CommandFailed(String),
    JsonParseError(String),
    NeighborNotFound,
    PermissionDenied(String),
    ProcessDead,
}

impl std::fmt::Display for TerminalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminalError::NotDetected => write!(f, "Terminal environment not detected"),
            TerminalError::CommandFailed(msg) => write!(f, "Command failed: {}", msg),
            TerminalError::JsonParseError(msg) => write!(f, "JSON parse error: {}", msg),
            TerminalError::NeighborNotFound => write!(f, "Adjacent terminal pane not found"),
            TerminalError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            TerminalError::ProcessDead => write!(f, "Target process is no longer alive"),
        }
    }
}

impl std::error::Error for TerminalError {}

pub trait TerminalAdapter: Send + Sync {
    fn terminal_name(&self) -> &'static str;
    fn is_available(&self) -> bool;
    fn detect_current_pane(&self) -> Result<String, TerminalError>;
    fn find_adjacent_agent(&self, current_pane: &str) -> Result<NeighborDetection, TerminalError>;
    fn get_agent_status(&self, pane_id: &str) -> Result<AgentStatus, TerminalError>;
    fn send_prompt(&self, target: &AgentTarget, prompt: &str) -> Result<DeliveryAck, TerminalError>;
    fn verify_target_alive(&self, target: &AgentTarget) -> Result<bool, TerminalError>;
}
