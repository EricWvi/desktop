//! Desktop agent runtime operations.

use super::run_async_backend;
use crate::{error::CommandError, state::DesktopState};
use ora_contracts::*;
use tauri::State;

backend_command!(
    get_agent_runtime_status,
    GetAgentRuntimeStatusRequest,
    GetAgentRuntimeStatusResponse,
    get_agent_runtime_status,
    "Reports the live detection status of every application-scoped CLI runtime through the shared Backend."
);

/// Lists the models one agent discovers for a workspace outside any session.
#[tauri::command]
pub async fn list_agent_models(
    state: State<'_, DesktopState>,
    request: ListAgentModelsRequest,
) -> Result<ListAgentModelsResponse, CommandError> {
    run_async_backend(
        "list_agent_models",
        state.backend.list_agent_models(request),
    )
    .await
}
