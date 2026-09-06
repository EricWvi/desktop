//! Desktop session operations.

use super::{run_async_backend, run_backend};
use crate::{error::CommandError, state::DesktopState};
use ora_backend::Backend;
use ora_contracts::*;
use tauri::State;

/// Starts history replay without embedding session creation in the shared transport dispatcher.
pub(super) async fn start_load(
    state: State<'_, DesktopState>,
    request: LoadSessionRequest,
    context: super::stream::StreamStart,
) -> Result<(), CommandError> {
    context.events(state.backend.load_session(request)).await
}

/// Starts a prompt using the session-owned backend while the context owns stream cancellation.
pub(super) async fn start_prompt(
    state: State<'_, DesktopState>,
    request: PromptSessionRequest,
    context: super::stream::StreamStart,
) -> Result<(), CommandError> {
    context.events(state.backend.prompt_session(request)).await
}

/// Creates and persists a provider session when a chat first sends.
#[tauri::command]
pub async fn start_session(
    state: State<'_, DesktopState>,
    request: StartSessionRequest,
) -> Result<StartSessionResponse, CommandError> {
    run_async_backend("start_session", state.backend.start_session(request)).await
}

/// Applies one configuration option to a persisted session.
#[tauri::command]
pub async fn set_session_config(
    state: State<'_, DesktopState>,
    request: SetSessionConfigRequest,
) -> Result<SetSessionConfigResponse, CommandError> {
    run_async_backend(
        "set_session_config",
        state.backend.set_session_config(request),
    )
    .await
}

backend_command!(
    get_session,
    GetSessionRequest,
    GetSessionResponse,
    get_session,
    "Gets one session through the shared Backend."
);
backend_command!(
    list_sessions,
    ListSessionsRequest,
    ListSessionsResponse,
    list_sessions,
    "Lists sessions through the shared Backend."
);
/// Routes one permission choice through the owning Session actor.
#[tauri::command]
pub async fn respond_to_session_permission(
    state: State<'_, DesktopState>,
    request: RespondToPermissionRequest,
) -> Result<RespondToPermissionResponse, CommandError> {
    run_async_backend(
        "respond_to_session_permission",
        state.backend.respond_to_session_permission(request),
    )
    .await
}

/// Stops one provider process while retaining the Ora session record.
#[tauri::command]
pub async fn stop_session(
    state: State<'_, DesktopState>,
    request: StopSessionRequest,
) -> Result<StopSessionResponse, CommandError> {
    run_async_backend("stop_session", state.backend.stop_session(request)).await
}

/// Cancels the active prompt without unloading the reusable session.
#[tauri::command]
pub async fn cancel_session_prompt(
    state: State<'_, DesktopState>,
    request: CancelSessionPromptRequest,
) -> Result<CancelSessionPromptResponse, CommandError> {
    run_backend(
        "cancel_session_prompt",
        state.backend.clone(),
        request,
        Backend::cancel_session_prompt,
    )
    .await
}

/// Moves one conversation onto a different agent CLI without changing its identity.
#[tauri::command]
pub async fn switch_session_agent(
    state: State<'_, DesktopState>,
    request: SwitchSessionAgentRequest,
) -> Result<SwitchSessionAgentResponse, CommandError> {
    run_async_backend(
        "switch_session_agent",
        state.backend.switch_session_agent(request),
    )
    .await
}

/// Returns a session whose history writes failed to a writable state.
#[tauri::command]
pub async fn resume_session_history(
    state: State<'_, DesktopState>,
    request: ResumeSessionHistoryRequest,
) -> Result<ResumeSessionHistoryResponse, CommandError> {
    run_async_backend(
        "resume_session_history",
        state.backend.resume_session_history(request),
    )
    .await
}

/// Stops the provider process before removing the Ora session record and its history.
#[tauri::command]
pub async fn delete_session(
    state: State<'_, DesktopState>,
    request: DeleteSessionRequest,
) -> Result<DeleteSessionResponse, CommandError> {
    run_async_backend("delete_session", state.backend.delete_session(request)).await
}

/// Renames one session through the shared Backend.
#[tauri::command]
pub async fn rename_session(
    state: State<'_, DesktopState>,
    request: RenameSessionRequest,
) -> Result<RenameSessionResponse, CommandError> {
    run_async_backend("rename_session", state.backend.rename_session(request)).await
}
