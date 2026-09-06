//! Desktop workflow operations.

use ora_contracts::*;
use serde::Deserialize;
use std::path::PathBuf;

/// Carries a user-selected destination and serialized workflow definition for export.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteWorkflowExportRequest {
    path: PathBuf,
    content: String,
}

/// Writes a workflow export after the desktop save dialog has selected its exact destination.
#[tauri::command]
pub async fn write_workflow_export(request: WriteWorkflowExportRequest) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || std::fs::write(request.path, request.content))
        .await
        .map_err(|error| format!("workflow export task failed: {error}"))?
        .map_err(|error| format!("workflow export write failed: {error}"))
}

backend_command!(
    create_workflow,
    CreateWorkflowRequest,
    CreateWorkflowResponse,
    create_workflow,
    "Creates one workflow through the shared Backend."
);
backend_command!(
    get_workflow,
    GetWorkflowRequest,
    GetWorkflowResponse,
    get_workflow,
    "Gets one workflow through the shared Backend."
);
backend_command!(
    list_workflows,
    ListWorkflowsRequest,
    ListWorkflowsResponse,
    list_workflows,
    "Lists workflows through the shared Backend."
);
backend_command!(
    update_workflow,
    UpdateWorkflowRequest,
    UpdateWorkflowResponse,
    update_workflow,
    "Updates one workflow through the shared Backend."
);
backend_command!(
    delete_workflow,
    DeleteWorkflowRequest,
    DeleteWorkflowResponse,
    delete_workflow,
    "Deletes one workflow through the shared Backend."
);
backend_command!(
    get_workflow_draft,
    GetDraftRequest,
    GetDraftResponse,
    get_workflow_draft,
    "Gets one workflow's draft snapshot through the shared Backend."
);
backend_command!(
    update_workflow_draft,
    UpdateDraftRequest,
    UpdateDraftResponse,
    update_workflow_draft,
    "Updates one workflow's draft graph through the shared Backend."
);
backend_command!(
    publish_workflow,
    PublishWorkflowRequest,
    PublishWorkflowResponse,
    publish_workflow,
    "Publishes one workflow draft through the shared Backend."
);
backend_command!(
    rollback_workflow,
    RollbackWorkflowRequest,
    RollbackWorkflowResponse,
    rollback_workflow,
    "Rolls back one workflow draft through the shared Backend."
);
backend_command!(
    activate_workflow,
    ActivateWorkflowRequest,
    ActivateWorkflowResponse,
    activate_workflow,
    "Activates one workflow version through the shared Backend."
);
backend_command!(
    list_workflow_versions,
    ListVersionsRequest,
    ListVersionsResponse,
    list_workflow_versions,
    "Lists one workflow's published versions through the shared Backend."
);
backend_command!(
    get_workflow_version,
    GetVersionRequest,
    GetVersionResponse,
    get_workflow_version,
    "Gets one workflow version snapshot through the shared Backend."
);
backend_command!(
    delete_workflow_snapshot,
    DeleteSnapshotRequest,
    DeleteSnapshotResponse,
    delete_workflow_snapshot,
    "Deletes one workflow snapshot through the shared Backend."
);
backend_command!(
    get_workflow_snapshot,
    GetWorkflowSnapshotRequest,
    GetWorkflowSnapshotResponse,
    get_workflow_snapshot,
    "Gets one snapshot by id through the shared Backend."
);
