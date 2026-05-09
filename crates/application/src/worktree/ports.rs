use ora_domain::{Worktree, WorktreeId};

/// Supplies application-owned persistence operations for worktree CRUD use cases.
///
/// Implementations are expected to hide storage details such as soft-delete columns
/// while preserving the transport-agnostic behavior required by the handlers.
pub trait WorktreeRepository {
    /// Persists a newly created worktree and returns the stored snapshot.
    fn create_worktree(&self, worktree: Worktree) -> Result<Worktree, WorktreeRepositoryError>;

    /// Loads one visible worktree by identifier.
    fn find_worktree(
        &self,
        worktree_id: &WorktreeId,
    ) -> Result<Option<Worktree>, WorktreeRepositoryError>;

    /// Lists every visible worktree in storage order.
    fn list_worktrees(&self) -> Result<Vec<Worktree>, WorktreeRepositoryError>;

    /// Persists a worktree replacement produced by the application layer.
    fn update_worktree(&self, worktree: Worktree) -> Result<Worktree, WorktreeRepositoryError>;

    /// Marks a worktree deleted and returns whether a visible worktree was affected.
    fn soft_delete_worktree(
        &self,
        worktree_id: &WorktreeId,
        deleted_at: i64,
    ) -> Result<bool, WorktreeRepositoryError>;
}

/// Supplies new worktree identifiers for create use cases.
pub trait WorktreeIdGenerator {
    /// Produces the identifier for a newly created worktree.
    fn generate_worktree_id(&self) -> WorktreeId;
}

/// Captures repository failures that handlers convert into stable application errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeRepositoryError {
    OperationFailed(String),
}
