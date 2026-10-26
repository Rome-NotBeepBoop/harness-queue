use serde::{Deserialize, Serialize};

pub const DEFAULT_CONTINUATION_PROMPT: &str =
    "Continue the unfinished work. Inspect the session history and repository state before making changes. Do not repeat completed tasks.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    AutoResume,
    AutoSend,
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::AutoResume => "Auto-resume",
            Mode::AutoSend => "Auto-send",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    Detecting,
    SelectingMode,
    Watching,
    RateLimited,
    WaitingForRecovery,
    RecoveringSession,
    AgentBusy,
    AgentReady,
    Submitting,
    PromptAcknowledged,
    DeliveryUncertain,
    Error,
    Detached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemStatus {
    Queued,
    Submitting,
    Acknowledged,
    Uncertain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueItem {
    pub id: usize,
    pub prompt: String,
    pub status: ItemStatus,
    pub queued_at: u64,
    pub submitted_at: Option<u64>,
}

impl QueueItem {
    pub fn new(id: usize, prompt: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            id,
            prompt,
            status: ItemStatus::Queued,
            queued_at: now,
            submitted_at: None,
        }
    }

    pub fn mark_submitting(&mut self, timestamp: u64) {
        self.status = ItemStatus::Submitting;
        self.submitted_at = Some(timestamp);
    }

    pub fn mark_acknowledged(&mut self) {
        self.status = ItemStatus::Acknowledged;
    }

    pub fn mark_uncertain(&mut self) {
        self.status = ItemStatus::Uncertain;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HarnessKind {
    Codex,
    ClaudeCode,
    OpenCode,
    Omp,
    Pi,
    Agy,
    Unknown(String),
}

impl HarnessKind {
    pub fn display_name(&self) -> &'static str {
        match self {
            HarnessKind::Codex => "OpenAI Codex",
            HarnessKind::ClaudeCode => "Claude Code",
            HarnessKind::OpenCode => "OpenCode",
            HarnessKind::Omp => "OMP",
            HarnessKind::Pi => "Pi",
            HarnessKind::Agy => "Google Antigravity",
            HarnessKind::Unknown(_) => "Unknown Agent",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTarget {
    pub terminal_id: String,
    pub pane_id: String,
    pub pid: u32,
    pub harness: HarnessKind,
    pub session_id: String,
    pub cwd: String,
}

impl AgentTarget {
    pub fn short_session_id(&self) -> &str {
        if let Some(pos) = self.session_id.find('-') {
            &self.session_id[..pos]
        } else if self.session_id.len() > 8 {
            &self.session_id[..8]
        } else {
            &self.session_id
        }
    }
}
