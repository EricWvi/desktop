use ora_contracts::{
    AgentCli as ContractAgentCli, Session as ContractSession,
    SessionStatus as ContractSessionStatus,
};
use ora_domain::{
    AgentCli as DomainAgentCli, Session as DomainSession, SessionStatus as DomainSessionStatus,
};

/// Maps a domain session into the app-facing contract shape.
pub(crate) fn map_session(session: DomainSession) -> ContractSession {
    ContractSession {
        id: session.id.to_string(),
        task_id: session.task_id.to_string(),
        agent_cli: map_agent_cli(session.agent_cli),
        status: map_session_status(session.status),
    }
}

/// Translates the provider CLI without leaking persistence representation.
fn map_agent_cli(agent_cli: DomainAgentCli) -> ContractAgentCli {
    match agent_cli {
        DomainAgentCli::OpenCode => ContractAgentCli::OpenCode,
        DomainAgentCli::Nga => ContractAgentCli::Nga,
        DomainAgentCli::CodeAgentCli => ContractAgentCli::CodeAgentCli,
    }
}

/// Translates the internal session status into the transport-facing enum.
fn map_session_status(status: DomainSessionStatus) -> ContractSessionStatus {
    match status {
        DomainSessionStatus::Running => ContractSessionStatus::Running,
        DomainSessionStatus::Stopped => ContractSessionStatus::Stopped,
    }
}
