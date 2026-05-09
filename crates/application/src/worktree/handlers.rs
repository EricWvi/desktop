use crate::worktree::mapper::map_worktree;
use crate::worktree::ports::{WorktreeIdGenerator, WorktreeRepository};
use crate::{ApplicationError, Clock};
use ora_contracts::{
    CreateWorktreeRequest, CreateWorktreeResponse, DeleteWorktreeRequest, DeleteWorktreeResponse,
    GetWorktreeRequest, GetWorktreeResponse, ListWorktreesRequest, ListWorktreesResponse,
    UpdateWorktreeRequest, UpdateWorktreeResponse, WorktreeActivity,
};
use ora_domain::{
    AuditFields, TaskId, Worktree as DomainWorktree, WorktreeActivity as DomainWorktreeActivity,
    WorktreeId,
};
use ora_logging::{ora_error, ora_info};

/// Handles worktree creation without depending on transport-specific concerns.
pub struct CreateWorktreeHandler<Repository, IdGenerator, ClockSource> {
    repository: Repository,
    id_generator: IdGenerator,
    clock: ClockSource,
}

impl<Repository, IdGenerator, ClockSource>
    CreateWorktreeHandler<Repository, IdGenerator, ClockSource>
{
    pub fn new(repository: Repository, id_generator: IdGenerator, clock: ClockSource) -> Self {
        Self {
            repository,
            id_generator,
            clock,
        }
    }
}

impl<Repository, IdGenerator, ClockSource>
    CreateWorktreeHandler<Repository, IdGenerator, ClockSource>
where
    Repository: WorktreeRepository,
    IdGenerator: WorktreeIdGenerator,
    ClockSource: Clock,
{
    /// Creates a new worktree snapshot and returns the public response payload.
    pub fn handle(
        &self,
        request: CreateWorktreeRequest,
    ) -> Result<CreateWorktreeResponse, ApplicationError> {
        let now = self.clock.now_timestamp_millis();
        let worktree = DomainWorktree::new(
            self.id_generator.generate_worktree_id(),
            TaskId::new(request.task_id),
            request.branch_name,
            map_contract_worktree_activity(request.activity),
            AuditFields::new(now, now, false),
        );
        let worktree = self.repository.create_worktree(worktree).map_err(|error| {
            let error = ApplicationError::from_worktree_repository_error(error);
            log_worktree_failure("create_worktree", None, &error);
            error
        })?;

        log_worktree_success("create_worktree", Some(&worktree.id));

        Ok(CreateWorktreeResponse {
            worktree: map_worktree(worktree),
        })
    }
}

/// Handles one worktree lookup without depending on transport-specific concerns.
pub struct GetWorktreeHandler<Repository> {
    repository: Repository,
}

impl<Repository> GetWorktreeHandler<Repository> {
    pub fn new(repository: Repository) -> Self {
        Self { repository }
    }
}

impl<Repository> GetWorktreeHandler<Repository>
where
    Repository: WorktreeRepository,
{
    /// Loads one visible worktree or returns a stable not-found application error.
    pub fn handle(
        &self,
        request: GetWorktreeRequest,
    ) -> Result<GetWorktreeResponse, ApplicationError> {
        let worktree_id = WorktreeId::new(request.worktree_id);
        let worktree = self
            .repository
            .find_worktree(&worktree_id)
            .map_err(|error| {
                let error = ApplicationError::from_worktree_repository_error(error);
                log_worktree_failure("get_worktree", Some(&worktree_id), &error);
                error
            })?;

        match worktree {
            Some(worktree) => {
                log_worktree_success("get_worktree", Some(&worktree_id));

                Ok(GetWorktreeResponse {
                    worktree: map_worktree(worktree),
                })
            }
            None => {
                let error = ApplicationError::WorktreeNotFound {
                    worktree_id: worktree_id.to_string(),
                };
                log_worktree_failure("get_worktree", Some(&worktree_id), &error);
                Err(error)
            }
        }
    }
}

/// Handles worktree listing without depending on transport-specific concerns.
pub struct ListWorktreesHandler<Repository> {
    repository: Repository,
}

impl<Repository> ListWorktreesHandler<Repository> {
    pub fn new(repository: Repository) -> Self {
        Self { repository }
    }
}

impl<Repository> ListWorktreesHandler<Repository>
where
    Repository: WorktreeRepository,
{
    /// Lists every visible worktree and maps each one into the shared contract view.
    pub fn handle(
        &self,
        _request: ListWorktreesRequest,
    ) -> Result<ListWorktreesResponse, ApplicationError> {
        let worktrees = self.repository.list_worktrees().map_err(|error| {
            let error = ApplicationError::from_worktree_repository_error(error);
            log_worktree_failure("list_worktrees", None, &error);
            error
        })?;

        ora_info!(
            message = "listed worktrees",
            operation = "list_worktrees",
            worktree_count = worktrees.len()
        );

        Ok(ListWorktreesResponse {
            worktrees: worktrees.into_iter().map(map_worktree).collect(),
        })
    }
}

/// Handles worktree updates without depending on transport-specific concerns.
pub struct UpdateWorktreeHandler<Repository, ClockSource> {
    repository: Repository,
    clock: ClockSource,
}

impl<Repository, ClockSource> UpdateWorktreeHandler<Repository, ClockSource> {
    pub fn new(repository: Repository, clock: ClockSource) -> Self {
        Self { repository, clock }
    }
}

impl<Repository, ClockSource> UpdateWorktreeHandler<Repository, ClockSource>
where
    Repository: WorktreeRepository,
    ClockSource: Clock,
{
    /// Replaces the public worktree fields while preserving persistence-managed audit state.
    pub fn handle(
        &self,
        request: UpdateWorktreeRequest,
    ) -> Result<UpdateWorktreeResponse, ApplicationError> {
        let worktree_id = WorktreeId::new(request.worktree_id);
        let existing_worktree = self
            .repository
            .find_worktree(&worktree_id)
            .map_err(|error| {
                let error = ApplicationError::from_worktree_repository_error(error);
                log_worktree_failure("update_worktree", Some(&worktree_id), &error);
                error
            })?;

        let existing_worktree = match existing_worktree {
            Some(existing_worktree) => existing_worktree,
            None => {
                let error = ApplicationError::WorktreeNotFound {
                    worktree_id: worktree_id.to_string(),
                };
                log_worktree_failure("update_worktree", Some(&worktree_id), &error);
                return Err(error);
            }
        };

        let worktree = DomainWorktree::new(
            worktree_id.clone(),
            TaskId::new(request.task_id),
            request.branch_name,
            map_contract_worktree_activity(request.activity),
            AuditFields::new(
                existing_worktree.audit_fields.created_at,
                self.clock.now_timestamp_millis(),
                existing_worktree.audit_fields.is_deleted,
            ),
        );
        let worktree = self.repository.update_worktree(worktree).map_err(|error| {
            let error = ApplicationError::from_worktree_repository_error(error);
            log_worktree_failure("update_worktree", Some(&worktree_id), &error);
            error
        })?;

        log_worktree_success("update_worktree", Some(&worktree_id));

        Ok(UpdateWorktreeResponse {
            worktree: map_worktree(worktree),
        })
    }
}

/// Handles worktree deletion without exposing storage-specific soft-delete semantics.
pub struct DeleteWorktreeHandler<Repository, ClockSource> {
    repository: Repository,
    clock: ClockSource,
}

impl<Repository, ClockSource> DeleteWorktreeHandler<Repository, ClockSource> {
    pub fn new(repository: Repository, clock: ClockSource) -> Self {
        Self { repository, clock }
    }
}

impl<Repository, ClockSource> DeleteWorktreeHandler<Repository, ClockSource>
where
    Repository: WorktreeRepository,
    ClockSource: Clock,
{
    /// Deletes one worktree through a CRUD-shaped contract while letting storage soft-delete it.
    pub fn handle(
        &self,
        request: DeleteWorktreeRequest,
    ) -> Result<DeleteWorktreeResponse, ApplicationError> {
        let worktree_id = WorktreeId::new(request.worktree_id);
        let deleted = self
            .repository
            .soft_delete_worktree(&worktree_id, self.clock.now_timestamp_millis())
            .map_err(|error| {
                let error = ApplicationError::from_worktree_repository_error(error);
                log_worktree_failure("delete_worktree", Some(&worktree_id), &error);
                error
            })?;

        if deleted {
            log_worktree_success("delete_worktree", Some(&worktree_id));

            Ok(DeleteWorktreeResponse {
                worktree_id: worktree_id.to_string(),
            })
        } else {
            let error = ApplicationError::WorktreeNotFound {
                worktree_id: worktree_id.to_string(),
            };
            log_worktree_failure("delete_worktree", Some(&worktree_id), &error);
            Err(error)
        }
    }
}

/// Emits the shared informational event shape for successful worktree CRUD operations.
fn log_worktree_success(operation: &'static str, worktree_id: Option<&WorktreeId>) {
    match worktree_id {
        Some(worktree_id) => {
            ora_info!(
                message = "worktree operation completed",
                operation,
                worktree_id = worktree_id.to_string()
            );
        }
        None => {
            ora_info!(message = "worktree operation completed", operation);
        }
    }
}

/// Emits the shared error event shape for failed worktree CRUD operations.
fn log_worktree_failure(
    operation: &'static str,
    worktree_id: Option<&WorktreeId>,
    error: &ApplicationError,
) {
    match (worktree_id, error) {
        (Some(worktree_id), ApplicationError::WorktreeNotFound { .. }) => {
            ora_error!(
                message = "worktree operation failed",
                operation,
                worktree_id = worktree_id.to_string(),
                error.kind = "worktree_not_found",
                error.message = error.to_string()
            );
        }
        (Some(worktree_id), ApplicationError::WorktreeRepository { .. }) => {
            ora_error!(
                message = "worktree operation failed",
                operation,
                worktree_id = worktree_id.to_string(),
                error.kind = "worktree_repository",
                error.message = error.to_string()
            );
        }
        (None, ApplicationError::WorktreeRepository { .. }) => {
            ora_error!(
                message = "worktree operation failed",
                operation,
                error.kind = "worktree_repository",
                error.message = error.to_string()
            );
        }
        (None, ApplicationError::WorktreeNotFound { .. }) => {
            ora_error!(
                message = "worktree operation failed",
                operation,
                error.kind = "worktree_not_found",
                error.message = error.to_string()
            );
        }
        _ => {}
    }
}

/// Translates the transport-facing worktree activity into the domain enum.
fn map_contract_worktree_activity(activity: WorktreeActivity) -> DomainWorktreeActivity {
    match activity {
        WorktreeActivity::Inactive => DomainWorktreeActivity::Inactive,
        WorktreeActivity::Active => DomainWorktreeActivity::Active,
    }
}
