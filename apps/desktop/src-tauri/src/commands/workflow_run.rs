//! Desktop workflow run operations.

use crate::{error::CommandError, state::DesktopState};
use ora_contracts::*;
use tauri::State;

backend_command!(
    create_workflow_run,
    CreateWorkflowRunRequest,
    CreateWorkflowRunResponse,
    create_workflow_run,
    "Creates one workflow run through the shared Backend."
);
backend_command!(
    get_workflow_run,
    GetWorkflowRunRequest,
    GetWorkflowRunResponse,
    get_workflow_run,
    "Gets one workflow run through the shared Backend."
);
backend_command!(
    list_workflow_runs,
    ListWorkflowRunsRequest,
    ListWorkflowRunsResponse,
    list_workflow_runs,
    "Lists workflow runs for one project through the shared Backend."
);
backend_command!(
    list_workflow_runs_by_workflow,
    ListWorkflowRunsByWorkflowRequest,
    ListWorkflowRunsByWorkflowResponse,
    list_workflow_runs_by_workflow,
    "Lists workflow runs for one workflow through the shared Backend."
);
backend_command!(
    list_workflow_node_runs,
    ListWorkflowNodeRunsRequest,
    ListWorkflowNodeRunsResponse,
    list_workflow_node_runs,
    "Lists the node-run history of one workflow run through the shared Backend."
);
backend_command!(
    delete_workflow_run,
    DeleteWorkflowRunRequest,
    DeleteWorkflowRunResponse,
    delete_workflow_run,
    "Deletes one workflow run through the shared Backend."
);
backend_command!(
    rename_workflow_run,
    RenameWorkflowRunRequest,
    RenameWorkflowRunResponse,
    rename_workflow_run,
    "Renames one workflow run through the shared Backend."
);
backend_command!(
    start_workflow_run,
    StartWorkflowRunRequest,
    StartWorkflowRunResponse,
    start_workflow_run,
    "Starts one workflow run through the shared Backend."
);
/// Cancels one workflow run through the shared Backend.
///
/// Not a `backend_command!` because cancelling also stops the run's live agent
/// sessions, which is asynchronous.
#[tauri::command]
pub async fn cancel_workflow_run(
    state: State<'_, DesktopState>,
    request: CancelWorkflowRunRequest,
) -> Result<CancelWorkflowRunResponse, CommandError> {
    state
        .backend
        .cancel_workflow_run(request)
        .await
        .map_err(CommandError::from)
}
backend_command!(
    restart_workflow_run,
    RestartWorkflowRunRequest,
    RestartWorkflowRunResponse,
    restart_workflow_run,
    "Restarts one workflow run through the shared Backend."
);
backend_command!(
    update_workflow_run_input,
    UpdateWorkflowRunInputRequest,
    UpdateWorkflowRunInputResponse,
    update_workflow_run_input,
    "Updates the kickoff input of one workflow run through the shared Backend."
);
/// Completes one awaiting interactive workflow node through the shared Backend.
///
/// Not a `backend_command!` because completion also stops the node's session, which is
/// asynchronous.
#[tauri::command]
pub async fn complete_workflow_node(
    state: State<'_, DesktopState>,
    request: CompleteWorkflowNodeRequest,
) -> Result<CompleteWorkflowNodeResponse, CommandError> {
    state
        .backend
        .complete_workflow_node(request)
        .await
        .map_err(CommandError::from)
}
