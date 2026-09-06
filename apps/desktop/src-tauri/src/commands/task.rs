//! Desktop task operations.

use crate::{error::CommandError, state::DesktopState};
use ora_contracts::*;
use tauri::State;

backend_command!(
    create_task,
    CreateTaskRequest,
    CreateTaskResponse,
    create_task,
    "Creates one task through the shared Backend."
);
backend_command!(
    get_task,
    GetTaskRequest,
    GetTaskResponse,
    get_task,
    "Gets one task through the shared Backend."
);
backend_command!(
    list_tasks,
    ListTasksRequest,
    ListTasksResponse,
    list_tasks,
    "Lists tasks through the shared Backend."
);
backend_command!(
    update_task,
    UpdateTaskRequest,
    UpdateTaskResponse,
    update_task,
    "Updates one task through the shared Backend."
);
/// Deletes one task through the shared Backend.
///
/// Not a `backend_command!` because deleting also returns the warm provider
/// session the Task owned, which is asynchronous.
#[tauri::command]
pub async fn delete_task(
    state: State<'_, DesktopState>,
    request: DeleteTaskRequest,
) -> Result<DeleteTaskResponse, CommandError> {
    state
        .backend
        .delete_task(request)
        .await
        .map_err(CommandError::from)
}

backend_command!(
    get_task_workspace,
    GetTaskWorkspaceRequest,
    GetTaskWorkspaceResponse,
    get_task_workspace,
    "Returns the authoritative task root and optional linked-worktree branch."
);
