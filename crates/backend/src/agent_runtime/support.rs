use crate::{BackendError, BackendErrorKind};
use gitlancer::git::worktree::ResolveWorktreeByBranchRequest;
use gitlancer::{CliGitRunner, Git, RepoRoot, Repository};
use ora_acp::AcpClient;
use ora_application::{ProjectRepository, TaskRepository, WorktreeRepository};
use ora_contracts::acp::permission::{
    PermissionOptionId, RequestPermissionOutcome, RequestPermissionResponse,
    SelectedPermissionOutcome,
};
use ora_contracts::{
    RespondToPermissionRequest, RespondToPermissionResponse, Session as ContractSession,
    SessionStatus as ContractSessionStatus,
};
use ora_db::{
    RepositoryPool, SqliteProjectRepository, SqliteTaskRepository, SqliteWorktreeRepository,
};
use ora_domain::{Session, SessionStatus, TaskId, WorktreeActivity};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::process::ChildStdin;

/// Resolves the authoritative task worktree path through persisted ownership and Git metadata.
pub(super) fn resolve_task_cwd(
    pool: &RepositoryPool,
    task_id: &TaskId,
) -> Result<PathBuf, BackendError> {
    let task = SqliteTaskRepository::new(pool.clone())
        .find_task(task_id)
        .map_err(|_| task_worktree_unavailable())?
        .ok_or_else(task_worktree_unavailable)?;
    let worktree_id = task.worktree_id.ok_or_else(task_worktree_unavailable)?;
    let worktree = SqliteWorktreeRepository::new(pool.clone())
        .find_worktree(&worktree_id)
        .map_err(|_| task_worktree_unavailable())?
        .ok_or_else(task_worktree_unavailable)?;
    if worktree.task_id != task.id || worktree.activity != WorktreeActivity::Active {
        return Err(task_worktree_unavailable());
    }
    let branch_name = worktree.branch_name.ok_or_else(task_worktree_unavailable)?;
    let project = SqliteProjectRepository::new(pool.clone())
        .find_project(&task.project_id)
        .map_err(|_| task_worktree_unavailable())?
        .ok_or_else(task_worktree_unavailable)?;
    let repository = Repository::new(RepoRoot::new(project.root_path));
    let resolved = Git::new(CliGitRunner)
        .resolve_worktree_by_branch(ResolveWorktreeByBranchRequest {
            repository: &repository,
            branch_name: &branch_name,
        })
        .map_err(|_| task_worktree_unavailable())?;
    let cwd = resolved.worktree_root().as_path().to_path_buf();
    if !cwd.is_dir() {
        return Err(task_worktree_unavailable());
    }
    Ok(cwd)
}

/// Responds to a pending permission after validating the public request ownership.
pub(super) async fn respond_permission(
    client: &AcpClient<ChildStdin>,
    request: RespondToPermissionRequest,
    permissions: &mut HashMap<String, (ora_contracts::acp::rpc::RequestId, Vec<String>)>,
) -> Result<RespondToPermissionResponse, BackendError> {
    let Some((request_id, options)) = permissions.remove(&request.permission_request_id) else {
        return Err(BackendError::new(
            BackendErrorKind::Conflict,
            "permission_request_not_pending",
            "permission request is not pending",
        ));
    };
    if !options.contains(&request.option_id) {
        permissions.insert(request.permission_request_id, (request_id, options));
        return Err(BackendError::new(
            BackendErrorKind::BadRequest,
            "permission_option_invalid",
            "permission option does not belong to this request",
        ));
    }
    let outcome = RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
        PermissionOptionId::new(request.option_id),
    ));
    client
        .respond(&request_id, &RequestPermissionResponse::new(outcome))
        .await
        .map_err(map_acp_error)?;
    Ok(RespondToPermissionResponse {})
}

/// Maps a private domain session into its frontend-safe view.
pub(super) fn contract_session(session: Session) -> ContractSession {
    ContractSession {
        id: session.id.to_string(),
        task_id: session.task_id.to_string(),
        status: match session.status {
            SessionStatus::Running => ContractSessionStatus::Running,
            SessionStatus::Stopped => ContractSessionStatus::Stopped,
        },
    }
}

/// Resolves OpenCode through the Windows executable lookup mechanism for each retry generation.
#[cfg(windows)]
pub(super) fn resolve_opencode_path(_home_directory: &Path) -> Result<PathBuf, BackendError> {
    let output = std::process::Command::new("where.exe")
        .arg("opencode")
        .output()
        .map_err(|_| runtime_internal("opencode_resolution_failed", "failed to run where.exe"))?;
    if !output.status.success() {
        return Err(runtime_internal(
            "opencode_not_found",
            "OpenCode executable not found on PATH",
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find(|line| {
            let lower = line.to_lowercase();
            lower.ends_with(".exe") || lower.ends_with(".cmd") || lower.ends_with(".bat")
        })
        .or_else(|| stdout.lines().next())
        .map(|path| PathBuf::from(path.trim()))
        .ok_or_else(|| {
            runtime_internal(
                "opencode_not_found",
                "OpenCode executable not found on PATH",
            )
        })
}

/// Resolves OpenCode from its fixed per-user Unix installation directory for each retry generation.
#[cfg(unix)]
pub(super) fn resolve_opencode_path(home_directory: &Path) -> Result<PathBuf, BackendError> {
    Ok(home_directory
        .join(".opencode")
        .join("bin")
        .join("opencode"))
}

/// Drains child stderr so provider diagnostics can never block the shared process.
pub(super) async fn drain_stderr(mut stderr: tokio::process::ChildStderr) {
    use tokio::io::AsyncReadExt;
    let mut tail = Vec::with_capacity(64 * 1024);
    let mut buffer = [0_u8; 4096];
    loop {
        match stderr.read(&mut buffer).await {
            Ok(0) | Err(_) => return,
            Ok(read) => {
                tail.extend_from_slice(&buffer[..read]);
                if tail.len() > 64 * 1024 {
                    tail.drain(..tail.len() - 64 * 1024);
                }
            }
        }
    }
}

/// Builds the stable public error for an unknown or deleted Ora session.
pub(super) fn session_not_found(session_id: &str) -> BackendError {
    BackendError::new(
        BackendErrorKind::NotFound,
        "session_not_found",
        format!("session not found: {session_id}"),
    )
}

/// Builds the conflict returned when a prompt targets an unloaded logical session.
pub(super) fn session_stopped() -> BackendError {
    BackendError::new(
        BackendErrorKind::Conflict,
        "session_stopped",
        "session must be loaded before prompting",
    )
}

/// Builds the degraded-mode error while OpenCode is starting or recovering.
pub(super) fn runtime_unavailable() -> BackendError {
    runtime_internal(
        "agent_runtime_unavailable",
        "OpenCode runtime is unavailable",
    )
}

/// Hides transport internals behind the backend's stable protocol error.
pub(super) fn map_acp_error(error: ora_acp::AcpError) -> BackendError {
    runtime_internal("agent_protocol_error", error.to_string())
}

/// Builds an internal runtime error with a caller-selected stable code.
pub(super) fn runtime_internal(code: &'static str, message: impl Into<String>) -> BackendError {
    BackendError::new(BackendErrorKind::Internal, code, message)
}

/// Builds the conflict used when task ownership cannot resolve an active Git worktree.
fn task_worktree_unavailable() -> BackendError {
    BackendError::new(
        BackendErrorKind::Conflict,
        "task_worktree_unavailable",
        "task worktree is unavailable",
    )
}

#[cfg(all(test, unix))]
mod tests {
    use super::resolve_opencode_path;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    /// Verifies Unix lookup remains relative to the injected user home.
    #[test]
    fn resolves_unix_opencode_path_from_home_directory() {
        let home_directory = PathBuf::from("users").join("demo");
        assert_eq!(
            resolve_opencode_path(&home_directory).expect("resolve OpenCode path"),
            home_directory
                .join(".opencode")
                .join("bin")
                .join("opencode")
        );
    }
}
