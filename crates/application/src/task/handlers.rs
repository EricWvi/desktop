use crate::task::mapper::map_task;
use crate::task::ports::{TaskIdGenerator, TaskRepository};
use crate::{ApplicationError, Clock};
use ora_contracts::{
    CreateTaskRequest, CreateTaskResponse, DeleteTaskRequest, DeleteTaskResponse, GetTaskRequest,
    GetTaskResponse, ListTasksRequest, ListTasksResponse, TaskStatus, UpdateTaskRequest,
    UpdateTaskResponse,
};
use ora_domain::{
    AuditFields, ProjectId, Task as DomainTask, TaskId, TaskStatus as DomainTaskStatus, WorktreeId,
};
use ora_logging::{ora_error, ora_info};

/// Handles task creation without depending on transport-specific concerns.
pub struct CreateTaskHandler<Repository, IdGenerator, ClockSource> {
    repository: Repository,
    id_generator: IdGenerator,
    clock: ClockSource,
}

impl<Repository, IdGenerator, ClockSource> CreateTaskHandler<Repository, IdGenerator, ClockSource> {
    pub fn new(repository: Repository, id_generator: IdGenerator, clock: ClockSource) -> Self {
        Self {
            repository,
            id_generator,
            clock,
        }
    }
}

impl<Repository, IdGenerator, ClockSource> CreateTaskHandler<Repository, IdGenerator, ClockSource>
where
    Repository: TaskRepository,
    IdGenerator: TaskIdGenerator,
    ClockSource: Clock,
{
    /// Creates a new task snapshot and returns the public response payload.
    pub fn handle(
        &self,
        request: CreateTaskRequest,
    ) -> Result<CreateTaskResponse, ApplicationError> {
        let now = self.clock.now_timestamp_millis();
        let task = DomainTask::new(
            self.id_generator.generate_task_id(),
            ProjectId::new(request.project_id),
            request.title,
            map_contract_task_status(request.status),
            request.worktree_id.map(WorktreeId::new),
            AuditFields::new(now, now, false),
        );
        let task = self.repository.create_task(task).map_err(|error| {
            let error = ApplicationError::from_task_repository_error(error);
            log_task_failure("create_task", None, &error);
            error
        })?;

        log_task_success("create_task", Some(&task.id));

        Ok(CreateTaskResponse {
            task: map_task(task),
        })
    }
}

/// Handles one task lookup without depending on transport-specific concerns.
pub struct GetTaskHandler<Repository> {
    repository: Repository,
}

impl<Repository> GetTaskHandler<Repository> {
    pub fn new(repository: Repository) -> Self {
        Self { repository }
    }
}

impl<Repository> GetTaskHandler<Repository>
where
    Repository: TaskRepository,
{
    /// Loads one visible task or returns a stable not-found application error.
    pub fn handle(&self, request: GetTaskRequest) -> Result<GetTaskResponse, ApplicationError> {
        let task_id = TaskId::new(request.task_id);
        let task = self.repository.find_task(&task_id).map_err(|error| {
            let error = ApplicationError::from_task_repository_error(error);
            log_task_failure("get_task", Some(&task_id), &error);
            error
        })?;

        match task {
            Some(task) => {
                log_task_success("get_task", Some(&task_id));

                Ok(GetTaskResponse {
                    task: map_task(task),
                })
            }
            None => {
                let error = ApplicationError::TaskNotFound {
                    task_id: task_id.to_string(),
                };
                log_task_failure("get_task", Some(&task_id), &error);
                Err(error)
            }
        }
    }
}

/// Handles task listing without depending on transport-specific concerns.
pub struct ListTasksHandler<Repository> {
    repository: Repository,
}

impl<Repository> ListTasksHandler<Repository> {
    pub fn new(repository: Repository) -> Self {
        Self { repository }
    }
}

impl<Repository> ListTasksHandler<Repository>
where
    Repository: TaskRepository,
{
    /// Lists every visible task and maps each one into the shared contract view.
    pub fn handle(
        &self,
        _request: ListTasksRequest,
    ) -> Result<ListTasksResponse, ApplicationError> {
        let tasks = self.repository.list_tasks().map_err(|error| {
            let error = ApplicationError::from_task_repository_error(error);
            log_task_failure("list_tasks", None, &error);
            error
        })?;

        ora_info!(
            message = "listed tasks",
            operation = "list_tasks",
            task_count = tasks.len()
        );

        Ok(ListTasksResponse {
            tasks: tasks.into_iter().map(map_task).collect(),
        })
    }
}

/// Handles task updates without depending on transport-specific concerns.
pub struct UpdateTaskHandler<Repository, ClockSource> {
    repository: Repository,
    clock: ClockSource,
}

impl<Repository, ClockSource> UpdateTaskHandler<Repository, ClockSource> {
    pub fn new(repository: Repository, clock: ClockSource) -> Self {
        Self { repository, clock }
    }
}

impl<Repository, ClockSource> UpdateTaskHandler<Repository, ClockSource>
where
    Repository: TaskRepository,
    ClockSource: Clock,
{
    /// Replaces the public task fields while preserving persistence-managed audit state.
    pub fn handle(
        &self,
        request: UpdateTaskRequest,
    ) -> Result<UpdateTaskResponse, ApplicationError> {
        let task_id = TaskId::new(request.task_id);
        let existing_task = self.repository.find_task(&task_id).map_err(|error| {
            let error = ApplicationError::from_task_repository_error(error);
            log_task_failure("update_task", Some(&task_id), &error);
            error
        })?;

        let existing_task = match existing_task {
            Some(existing_task) => existing_task,
            None => {
                let error = ApplicationError::TaskNotFound {
                    task_id: task_id.to_string(),
                };
                log_task_failure("update_task", Some(&task_id), &error);
                return Err(error);
            }
        };

        let task = DomainTask::new(
            task_id.clone(),
            ProjectId::new(request.project_id),
            request.title,
            map_contract_task_status(request.status),
            request.worktree_id.map(WorktreeId::new),
            AuditFields::new(
                existing_task.audit_fields.created_at,
                self.clock.now_timestamp_millis(),
                existing_task.audit_fields.is_deleted,
            ),
        );
        let task = self.repository.update_task(task).map_err(|error| {
            let error = ApplicationError::from_task_repository_error(error);
            log_task_failure("update_task", Some(&task_id), &error);
            error
        })?;

        log_task_success("update_task", Some(&task_id));

        Ok(UpdateTaskResponse {
            task: map_task(task),
        })
    }
}

/// Handles task deletion without exposing storage-specific soft-delete semantics.
pub struct DeleteTaskHandler<Repository, ClockSource> {
    repository: Repository,
    clock: ClockSource,
}

impl<Repository, ClockSource> DeleteTaskHandler<Repository, ClockSource> {
    pub fn new(repository: Repository, clock: ClockSource) -> Self {
        Self { repository, clock }
    }
}

impl<Repository, ClockSource> DeleteTaskHandler<Repository, ClockSource>
where
    Repository: TaskRepository,
    ClockSource: Clock,
{
    /// Deletes one task through a CRUD-shaped contract while letting storage soft-delete it.
    pub fn handle(
        &self,
        request: DeleteTaskRequest,
    ) -> Result<DeleteTaskResponse, ApplicationError> {
        let task_id = TaskId::new(request.task_id);
        let deleted = self
            .repository
            .soft_delete_task(&task_id, self.clock.now_timestamp_millis())
            .map_err(|error| {
                let error = ApplicationError::from_task_repository_error(error);
                log_task_failure("delete_task", Some(&task_id), &error);
                error
            })?;

        if deleted {
            log_task_success("delete_task", Some(&task_id));

            Ok(DeleteTaskResponse {
                task_id: task_id.to_string(),
            })
        } else {
            let error = ApplicationError::TaskNotFound {
                task_id: task_id.to_string(),
            };
            log_task_failure("delete_task", Some(&task_id), &error);
            Err(error)
        }
    }
}

/// Emits the shared informational event shape for successful task CRUD operations.
fn log_task_success(operation: &'static str, task_id: Option<&TaskId>) {
    match task_id {
        Some(task_id) => {
            ora_info!(
                message = "task operation completed",
                operation,
                task_id = task_id.to_string()
            );
        }
        None => {
            ora_info!(message = "task operation completed", operation);
        }
    }
}

/// Emits the shared error event shape for failed task CRUD operations.
fn log_task_failure(operation: &'static str, task_id: Option<&TaskId>, error: &ApplicationError) {
    match (task_id, error) {
        (Some(task_id), ApplicationError::TaskNotFound { .. }) => {
            ora_error!(
                message = "task operation failed",
                operation,
                task_id = task_id.to_string(),
                error.kind = "task_not_found",
                error.message = error.to_string()
            );
        }
        (Some(task_id), ApplicationError::TaskRepository { .. }) => {
            ora_error!(
                message = "task operation failed",
                operation,
                task_id = task_id.to_string(),
                error.kind = "task_repository",
                error.message = error.to_string()
            );
        }
        (None, ApplicationError::TaskRepository { .. }) => {
            ora_error!(
                message = "task operation failed",
                operation,
                error.kind = "task_repository",
                error.message = error.to_string()
            );
        }
        (None, ApplicationError::TaskNotFound { .. }) => {
            ora_error!(
                message = "task operation failed",
                operation,
                error.kind = "task_not_found",
                error.message = error.to_string()
            );
        }
        _ => {}
    }
}

/// Translates the transport-facing task status into the domain enum.
fn map_contract_task_status(status: TaskStatus) -> DomainTaskStatus {
    match status {
        TaskStatus::Todo => DomainTaskStatus::Todo,
        TaskStatus::Doing => DomainTaskStatus::Doing,
        TaskStatus::Done => DomainTaskStatus::Done,
    }
}
