use crate::models::State;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    InvalidTransition { from: State, to: State },
}

impl std::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransitionError::InvalidTransition { from, to } => {
                write!(f, "Invalid state transition from {:?} to {:?}", from, to)
            }
        }
    }
}

impl std::error::Error for TransitionError {}

#[derive(Debug, Clone)]
pub struct StateMachine {
    current: State,
    previous: Option<State>,
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl StateMachine {
    pub fn new() -> Self {
        Self {
            current: State::Detecting,
            previous: None,
        }
    }

    pub fn current(&self) -> State {
        self.current
    }

    pub fn previous(&self) -> Option<State> {
        self.previous
    }

    pub fn can_dispatch(&self) -> bool {
        self.current == State::AgentReady
    }

    pub fn is_rate_limited(&self) -> bool {
        matches!(
            self.current,
            State::RateLimited | State::WaitingForRecovery | State::RecoveringSession
        )
    }

    pub fn is_busy(&self) -> bool {
        matches!(self.current, State::AgentBusy | State::Submitting)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.current, State::Error | State::Detached)
    }

    pub fn transition(&mut self, next: State) -> Result<(), TransitionError> {
        if self.is_valid_transition(self.current, next) {
            self.previous = Some(self.current);
            self.current = next;
            Ok(())
        } else {
            Err(TransitionError::InvalidTransition {
                from: self.current,
                to: next,
            })
        }
    }

    fn is_valid_transition(&self, from: State, to: State) -> bool {
        if to == State::Detached || to == State::Error {
            return true;
        }

        match from {
            State::Detecting => matches!(to, State::SelectingMode),
            State::SelectingMode => matches!(to, State::Watching | State::AgentReady | State::AgentBusy),
            State::Watching => matches!(
                to,
                State::AgentReady
                    | State::AgentBusy
                    | State::RateLimited
                    | State::WaitingForRecovery
            ),
            State::RateLimited => matches!(to, State::WaitingForRecovery | State::RecoveringSession | State::AgentReady),
            State::WaitingForRecovery => matches!(to, State::RecoveringSession | State::AgentReady),
            State::RecoveringSession => matches!(to, State::AgentReady | State::Watching),
            State::AgentReady => matches!(to, State::Submitting | State::AgentBusy | State::RateLimited | State::Watching),
            State::Submitting => matches!(to, State::PromptAcknowledged | State::DeliveryUncertain | State::AgentBusy | State::Error),
            State::PromptAcknowledged => matches!(to, State::AgentBusy | State::AgentReady | State::Watching),
            State::DeliveryUncertain => matches!(to, State::AgentReady | State::AgentBusy | State::Watching),
            State::AgentBusy => matches!(to, State::AgentReady | State::RateLimited | State::Watching),
            State::Error | State::Detached => false,
        }
    }
}
