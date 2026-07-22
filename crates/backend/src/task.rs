use crate::clock::SystemClock;
use crate::{BackendError, BackendErrorKind};
use ora_application::{
    ApplicationError, Clock, CreateTaskHandler, GetTaskHandler, GitTaskWorktreeProvisioner,
    ListTasksHandler, ProjectRepository, ProjectRepositoryError, UpdateTaskHandler,
    UuidTaskIdGenerator, UuidWorktreeIdGenerator,
};
use ora_contracts::{
    CreateTaskRequest, CreateTaskResponse, DeleteTaskRequest, DeleteTaskResponse, GetTaskRequest,
    GetTaskResponse, ListTasksRequest, ListTasksResponse, UpdateTaskRequest, UpdateTaskResponse,
};
use ora_db::{
    CascadeDeleteOutcome, RepositoryPool, SqliteCascadeRepository, SqliteProjectRepository,
    SqliteTaskRepository, SqliteWorktreeRepository,
};
use ora_domain::{Project, ProjectId, TaskId};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// Groups task handlers while resolving each Git repository from the task's owning project.
pub(crate) struct TaskApi {
    pool: RepositoryPool,
    worktree_root: Arc<RwLock<PathBuf>>,
    get: GetTaskHandler<SqliteTaskRepository>,
    list: ListTasksHandler<SqliteTaskRepository>,
    update: UpdateTaskHandler<SqliteTaskRepository, SystemClock>,
    clock: SystemClock,
}

impl TaskApi {
    /// Builds task handlers from shared persistence and mutable runtime path configuration.
    pub(crate) fn new(
        pool: RepositoryPool,
        worktree_root: Arc<RwLock<PathBuf>>,
        clock: SystemClock,
    ) -> Self {
        let repository = SqliteTaskRepository::new(pool.clone());

        Self {
            pool,
            worktree_root,
            get: GetTaskHandler::new(repository.clone()),
            list: ListTasksHandler::new(repository.clone()),
            update: UpdateTaskHandler::new(repository, clock),
            clock,
        }
    }

    /// Resolves the requested project and creates its task in the matching Git repository.
    pub(crate) fn create(
        &self,
        request: CreateTaskRequest,
    ) -> Result<CreateTaskResponse, ApplicationError> {
        let project = self.find_project(&ProjectId::new(&request.project_id))?;
        let task_repository = SqliteTaskRepository::new(self.pool.clone());
        let worktree_repository = SqliteWorktreeRepository::new(self.pool.clone());
        let handler = CreateTaskHandler::new(
            task_repository,
            worktree_repository,
            UuidTaskIdGenerator::new(),
            UuidWorktreeIdGenerator::new(),
            GitTaskWorktreeProvisioner::new(PathBuf::from(project.root_path)),
            self.worktree_root_snapshot()?,
            self.clock,
        );

        handler.handle(request)
    }

    /// Executes one task lookup through the application handler.
    pub(crate) fn get(&self, request: GetTaskRequest) -> Result<GetTaskResponse, ApplicationError> {
        self.get.handle(request)
    }

    /// Executes task listing through the application handler.
    pub(crate) fn list(
        &self,
        request: ListTasksRequest,
    ) -> Result<ListTasksResponse, ApplicationError> {
        self.list.handle(request)
    }

    /// Executes task replacement while preserving its owning project.
    pub(crate) fn update(
        &self,
        request: UpdateTaskRequest,
    ) -> Result<UpdateTaskResponse, ApplicationError> {
        self.update.handle(request)
    }

    /// Soft-deletes the task and Ora worktree record without touching Git state.
    pub(crate) fn delete(
        &self,
        request: DeleteTaskRequest,
    ) -> Result<DeleteTaskResponse, BackendError> {
        let task_id = TaskId::new(request.task_id);
        let outcome = SqliteCascadeRepository::new(self.pool.clone())
            .delete_task(&task_id, self.clock.now_timestamp_millis())
            .map_err(|_| {
                BackendError::new(
                    BackendErrorKind::Internal,
                    "task_repository_error",
                    "task repository operation failed",
                )
            })?;

        match outcome {
            CascadeDeleteOutcome::Deleted => Ok(DeleteTaskResponse {
                task_id: task_id.to_string(),
            }),
            CascadeDeleteOutcome::NotFound => Err(BackendError::new(
                BackendErrorKind::NotFound,
                "task_not_found",
                format!("task not found: {task_id}"),
            )),
            CascadeDeleteOutcome::ActiveSession => Err(BackendError::new(
                BackendErrorKind::Conflict,
                "resource_in_use",
                "task has a running session and cannot be deleted",
            )),
        }
    }

    /// Loads a visible project or returns the same stable not-found error as project handlers.
    fn find_project(&self, project_id: &ProjectId) -> Result<Project, ApplicationError> {
        let repository = SqliteProjectRepository::new(self.pool.clone());
        let project = repository
            .find_project(project_id)
            .map_err(project_repository_error)?;

        project.ok_or_else(|| ApplicationError::ProjectNotFound {
            project_id: project_id.to_string(),
        })
    }

    /// Captures the configured creation root once so an in-flight operation remains coherent.
    fn worktree_root_snapshot(&self) -> Result<PathBuf, ApplicationError> {
        self.worktree_root
            .read()
            .map(|root| root.clone())
            .map_err(|_| ApplicationError::TaskWorktree {
                message: "worktree root configuration is unavailable".to_string(),
            })
    }
}

/// Converts project repository failures encountered during dynamic task routing.
fn project_repository_error(error: ProjectRepositoryError) -> ApplicationError {
    match error {
        ProjectRepositoryError::OperationFailed(message) => {
            ApplicationError::ProjectRepository { message }
        }
    }
}
