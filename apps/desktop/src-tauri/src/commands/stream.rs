//! Desktop stream operations.

use crate::stream_forwarding::{forward_contract_stream, forward_workspace_watch};
use crate::workspace_files::workspace_file_backend_error;
use crate::{error::CommandError, state::DesktopState};
use ora_backend::{BackendError, RequestLifecycle, UuidRequestIdGenerator};
use ora_contracts::*;
use std::path::PathBuf;
use tauri::{State, ipc::Channel};
use tokio_util::sync::CancellationToken;

/// Starts one typed Session stream and forwards private transport frames over a Tauri Channel.
#[tauri::command]
pub async fn stream_contract(
    state: State<'_, DesktopState>,
    operation_name: String,
    request: serde_json::Value,
    stream_call_id: String,
    on_event: Channel<serde_json::Value>,
) -> Result<(), CommandError> {
    let lifecycle = RequestLifecycle::start(
        format!("stream_contract:{operation_name}"),
        &UuidRequestIdGenerator,
    );
    let cancellation = CancellationToken::new();

    match operation_name.as_str() {
        "loadSession" => {
            let request =
                serde_json::from_value::<LoadSessionRequest>(request).map_err(|source| {
                    CommandError::from_backend_with_lifecycle(
                        BackendError::internal("failed to decode stream request", source),
                        &lifecycle,
                    )
                })?;
            let stream =
                state.backend.load_session(request).await.map_err(|error| {
                    CommandError::from_backend_with_lifecycle(error, &lifecycle)
                })?;
            register_contract_stream(&state, &stream_call_id, &cancellation)
                .map_err(|error| CommandError::from_backend_with_lifecycle(error, &lifecycle))?;
            let registry = state.stream_cancellations.clone();
            tauri::async_runtime::spawn(forward_contract_stream(
                stream,
                cancellation,
                stream_call_id,
                registry,
                on_event,
                lifecycle,
            ));
        }
        "promptSession" => {
            let request =
                serde_json::from_value::<PromptSessionRequest>(request).map_err(|source| {
                    CommandError::from_backend_with_lifecycle(
                        BackendError::internal("failed to decode stream request", source),
                        &lifecycle,
                    )
                })?;
            let stream = state
                .backend
                .prompt_session(request)
                .await
                .map_err(|error| CommandError::from_backend_with_lifecycle(error, &lifecycle))?;
            register_contract_stream(&state, &stream_call_id, &cancellation)
                .map_err(|error| CommandError::from_backend_with_lifecycle(error, &lifecycle))?;
            let registry = state.stream_cancellations.clone();
            tauri::async_runtime::spawn(forward_contract_stream(
                stream,
                cancellation,
                stream_call_id,
                registry,
                on_event,
                lifecycle,
            ));
        }
        "watchAppEvents" => {
            let stream = state.backend.watch_app_events();
            register_contract_stream(&state, &stream_call_id, &cancellation)
                .map_err(|error| CommandError::from_backend_with_lifecycle(error, &lifecycle))?;
            let registry = state.stream_cancellations.clone();
            tauri::async_runtime::spawn(forward_contract_stream(
                stream,
                cancellation,
                stream_call_id,
                registry,
                on_event,
                lifecycle,
            ));
        }
        "watchWorkspace" => {
            let request =
                serde_json::from_value::<WatchWorkspaceRequest>(request).map_err(|source| {
                    CommandError::from_backend_with_lifecycle(
                        BackendError::internal("failed to decode stream request", source),
                        &lifecycle,
                    )
                })?;
            let task_id = request.task_id;
            let backend = state.backend.clone();
            let root =
                tauri::async_runtime::spawn_blocking(move || backend.resolve_task_cwd(&task_id))
                    .await
                    .map_err(|source| {
                        CommandError::from_backend_with_lifecycle(
                            BackendError::internal(
                                "Desktop workspace root resolution failed",
                                source,
                            ),
                            &lifecycle,
                        )
                    })?
                    .map_err(|error| {
                        CommandError::from_backend_with_lifecycle(error, &lifecycle)
                    })?;
            start_workspace_watch(
                state,
                root,
                stream_call_id,
                on_event,
                lifecycle,
                cancellation,
            )
            .await?;
        }
        "watchProject" => {
            let request =
                serde_json::from_value::<WatchProjectRequest>(request).map_err(|source| {
                    CommandError::from_backend_with_lifecycle(
                        BackendError::internal("failed to decode stream request", source),
                        &lifecycle,
                    )
                })?;
            let project_id = request.project_id;
            let backend = state.backend.clone();
            let root = tauri::async_runtime::spawn_blocking(move || {
                backend.resolve_project_cwd(&project_id)
            })
            .await
            .map_err(|source| {
                CommandError::from_backend_with_lifecycle(
                    BackendError::internal("Desktop workspace location resolution failed", source),
                    &lifecycle,
                )
            })?
            .map_err(|error| CommandError::from_backend_with_lifecycle(error, &lifecycle))?;
            start_workspace_watch(
                state,
                root,
                stream_call_id,
                on_event,
                lifecycle,
                cancellation,
            )
            .await?;
        }
        _ => {
            return Err(CommandError::from_backend_with_lifecycle(
                BackendError::new(
                    ora_backend::ErrorClassification::InvalidRequest,
                    PublicError::InvalidRequest(EmptyErrorParams {}),
                    "unsupported stream operation",
                ),
                &lifecycle,
            ));
        }
    }
    Ok(())
}

/// Starts a native filesystem watcher for an already-resolved checkout root.
async fn start_workspace_watch(
    state: State<'_, DesktopState>,
    root: PathBuf,
    stream_call_id: String,
    on_event: Channel<serde_json::Value>,
    lifecycle: RequestLifecycle,
    cancellation: CancellationToken,
) -> Result<(), CommandError> {
    let workspace_files = state.workspace_files.clone();
    let watcher = tauri::async_runtime::spawn_blocking(move || workspace_files.watch(&root))
        .await
        .map_err(|source| {
            CommandError::from_backend_with_lifecycle(
                BackendError::internal("Desktop workspace watcher setup failed", source),
                &lifecycle,
            )
        })?
        .map_err(|error| {
            CommandError::from_backend_with_lifecycle(
                workspace_file_backend_error(error),
                &lifecycle,
            )
        })?;
    register_contract_stream(&state, &stream_call_id, &cancellation)
        .map_err(|error| CommandError::from_backend_with_lifecycle(error, &lifecycle))?;
    let registry = state.stream_cancellations.clone();
    tauri::async_runtime::spawn(forward_workspace_watch(
        watcher,
        cancellation,
        stream_call_id,
        registry,
        on_event,
        lifecycle,
    ));
    Ok(())
}

/// Registers a successfully-created stream and rejects duplicate private call identifiers.
pub(crate) fn register_contract_stream(
    state: &DesktopState,
    stream_call_id: &str,
    cancellation: &CancellationToken,
) -> Result<(), BackendError> {
    let mut registrations = state.stream_cancellations.lock().map_err(|_poisoned| {
        BackendError::internal(
            "stream registration state is unavailable",
            std::io::Error::other("stream registration lock poisoned"),
        )
    })?;
    if registrations.contains_key(stream_call_id) {
        return Err(BackendError::new(
            ora_backend::ErrorClassification::Conflict,
            PublicError::InvalidRequest(EmptyErrorParams {}),
            "stream call id is already registered",
        ));
    }
    registrations.insert(stream_call_id.to_string(), cancellation.clone());
    Ok(())
}

/// Cancels one private stream registration without exposing its id as a business identifier.
#[tauri::command]
pub async fn cancel_contract_stream(
    state: State<'_, DesktopState>,
    stream_call_id: String,
) -> Result<(), CommandError> {
    let lifecycle = RequestLifecycle::start("cancel_contract_stream", &UuidRequestIdGenerator);
    let request_span =
        ora_logging::span_with_request_id("tauri_command", &lifecycle.request_id().to_string());

    // The body holds a registry lock and never awaits, so the span is entered with `in_scope`
    // instead of `Instrument`, which would keep a guard alive across the async fn boundary.
    request_span.in_scope(|| {
        let registration = state
            .stream_cancellations
            .lock()
            .map(|mut registrations| registrations.remove(&stream_call_id))
            .map_err(|_poisoned| {
                CommandError::from_backend_with_lifecycle(
                    BackendError::internal(
                        "stream registration state is unavailable",
                        std::io::Error::other("stream registration lock poisoned"),
                    ),
                    &lifecycle,
                )
            })?;

        // Cancelling an already-finished stream is not an error: the forwarding task removes its
        // own registration on completion, so a missing entry only means the race resolved first.
        if let Some(cancellation) = registration {
            cancellation.cancel();
        }

        lifecycle.complete_success();
        Ok(())
    })
}
