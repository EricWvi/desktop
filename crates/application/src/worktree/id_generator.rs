use crate::worktree::ports::WorktreeIdGenerator;
use ora_domain::WorktreeId;
use uuid::Uuid;

/// Generates worktree identifiers as random UUID v4 values.
#[derive(Clone, Copy, Debug, Default)]
pub struct UuidWorktreeIdGenerator;

impl UuidWorktreeIdGenerator {
    /// Creates a UUID-backed worktree identifier generator.
    pub fn new() -> Self {
        Self
    }
}

impl WorktreeIdGenerator for UuidWorktreeIdGenerator {
    /// Produces a fresh UUID v4 worktree identifier for create requests.
    fn generate_worktree_id(&self) -> WorktreeId {
        WorktreeId::new(Uuid::new_v4().to_string())
    }
}
