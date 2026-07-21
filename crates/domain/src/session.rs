use crate::{AuditFields, DomainModelError, TaskId};
use serde::{Deserialize, Serialize};

/// Identifies the provider CLI whose persistent ACP session owns the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentCli {
    OpenCode,
    Nga,
    CodeAgentCli,
}

impl AgentCli {
    /// Returns the stable integer stored by the baseline SQLite schema.
    pub fn database_value(self) -> i64 {
        match self {
            Self::OpenCode => 0,
            Self::Nga => 1,
            Self::CodeAgentCli => 2,
        }
    }

    /// Restores a CLI selection while rejecting unknown persisted values.
    pub fn from_database_value(value: i64) -> Result<Self, DomainModelError> {
        match value {
            0 => Ok(Self::OpenCode),
            1 => Ok(Self::Nga),
            2 => Ok(Self::CodeAgentCli),
            _ => Err(DomainModelError::InvalidAgentCli(value)),
        }
    }
}

/// Captures whether an agent process is currently running for this session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    Running,
    Stopped,
}

impl SessionStatus {
    /// Returns the integer code used by persistence adapters for this session status.
    pub fn database_value(self) -> i64 {
        match self {
            Self::Running => 0,
            Self::Stopped => 1,
        }
    }

    /// Converts a persisted integer into a strongly typed session status.
    pub fn from_database_value(value: i64) -> Result<Self, DomainModelError> {
        match value {
            0 => Ok(Self::Running),
            1 => Ok(Self::Stopped),
            _ => Err(DomainModelError::InvalidSessionStatus(value)),
        }
    }
}

/// Represents one provider-backed conversation whose routing fields are immutable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: crate::SessionId,
    pub task_id: TaskId,
    pub agent_cli: AgentCli,
    pub agent_session_id: String,
    pub status: SessionStatus,
    pub audit_fields: AuditFields,
}

impl Session {
    /// Creates a session only after the provider has returned its required session identifier.
    pub fn new(
        id: crate::SessionId,
        task_id: TaskId,
        agent_cli: AgentCli,
        agent_session_id: impl Into<String>,
        status: SessionStatus,
        audit_fields: AuditFields,
    ) -> Self {
        Self {
            id,
            task_id,
            agent_cli,
            agent_session_id: agent_session_id.into(),
            status,
            audit_fields,
        }
    }

    /// Changes only process lifecycle state while preserving immutable provider routing.
    pub fn with_status(mut self, status: SessionStatus, updated_at: i64) -> Self {
        self.status = status;
        self.audit_fields.updated_at = updated_at;
        self
    }
}
