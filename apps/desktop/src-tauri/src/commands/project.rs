//! Desktop project operations.

use crate::{error::CommandError, state::DesktopState};
use ora_contracts::*;
use tauri::State;

backend_command!(
    create_project,
    CreateProjectRequest,
    CreateProjectResponse,
    create_project,
    "Creates one project through the shared Backend."
);
backend_command!(
    get_project,
    GetProjectRequest,
    GetProjectResponse,
    get_project,
    "Gets one project through the shared Backend."
);
backend_command!(
    list_projects,
    ListProjectsRequest,
    ListProjectsResponse,
    list_projects,
    "Lists projects through the shared Backend."
);
backend_command!(
    list_project_branches,
    ListProjectBranchesRequest,
    ListProjectBranchesResponse,
    list_project_branches,
    "Lists local branches for one project through the shared Backend."
);
backend_command!(
    update_project,
    UpdateProjectRequest,
    UpdateProjectResponse,
    update_project,
    "Updates one project through the shared Backend."
);
/// Deletes one project through the shared Backend.
///
/// Not a `backend_command!` because deleting also returns the warm provider
/// sessions the project owned, which is asynchronous.
#[tauri::command]
pub async fn delete_project(
    state: State<'_, DesktopState>,
    request: DeleteProjectRequest,
) -> Result<DeleteProjectResponse, CommandError> {
    state
        .backend
        .delete_project(request)
        .await
        .map_err(CommandError::from)
}
