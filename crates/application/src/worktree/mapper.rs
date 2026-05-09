use ora_contracts::{Worktree as ContractWorktree, WorktreeActivity as ContractWorktreeActivity};
use ora_domain::{Worktree as DomainWorktree, WorktreeActivity as DomainWorktreeActivity};

/// Maps a domain worktree into the app-facing contract shape.
pub(crate) fn map_worktree(worktree: DomainWorktree) -> ContractWorktree {
    ContractWorktree {
        id: worktree.id.to_string(),
        task_id: worktree.task_id.to_string(),
        branch_name: worktree.branch_name,
        activity: map_worktree_activity(worktree.activity),
    }
}

/// Translates the internal worktree activity into the transport-facing enum.
fn map_worktree_activity(activity: DomainWorktreeActivity) -> ContractWorktreeActivity {
    match activity {
        DomainWorktreeActivity::Inactive => ContractWorktreeActivity::Inactive,
        DomainWorktreeActivity::Active => ContractWorktreeActivity::Active,
    }
}
