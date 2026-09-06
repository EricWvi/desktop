//! Desktop agent operations.

use ora_contracts::*;

backend_command!(
    create_agent,
    CreateAgentRequest,
    CreateAgentResponse,
    create_agent,
    "Creates one configurable agent through the shared Backend."
);
backend_command!(
    get_agent,
    GetAgentRequest,
    GetAgentResponse,
    get_agent,
    "Gets one configurable agent through the shared Backend."
);
backend_command!(
    list_agents,
    ListAgentsRequest,
    ListAgentsResponse,
    list_agents,
    "Lists configurable agents through the shared Backend."
);
backend_command!(
    update_agent,
    UpdateAgentRequest,
    UpdateAgentResponse,
    update_agent,
    "Updates one configurable agent through the shared Backend."
);
backend_command!(
    delete_agent,
    DeleteAgentRequest,
    DeleteAgentResponse,
    delete_agent,
    "Deletes one configurable agent through the shared Backend."
);

backend_command!(
    prepare_agent_import,
    PrepareAgentImportRequest,
    PrepareAgentImportResponse,
    prepare_agent_import,
    "Prepares one agent Markdown import source."
);
backend_command!(
    commit_agent_import,
    CommitAgentImportRequest,
    CommitAgentImportResponse,
    commit_agent_import,
    "Commits one prepared agent Markdown import."
);
