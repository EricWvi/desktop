use ora_domain::{Task, TaskId};

/// Supplies application-owned persistence operations for task CRUD use cases.
///
/// Implementations are expected to hide storage details such as soft-delete columns
/// while preserving the transport-agnostic behavior required by the handlers.
pub trait TaskRepository {
    /// Persists a newly created task and returns the stored snapshot.
    fn create_task(&self, task: Task) -> Result<Task, TaskRepositoryError>;

    /// Loads one visible task by identifier.
    fn find_task(&self, task_id: &TaskId) -> Result<Option<Task>, TaskRepositoryError>;

    /// Lists every visible task in storage order.
    fn list_tasks(&self) -> Result<Vec<Task>, TaskRepositoryError>;

    /// Persists a task replacement produced by the application layer.
    fn update_task(&self, task: Task) -> Result<Task, TaskRepositoryError>;

    /// Marks a task deleted and returns whether a visible task was affected.
    fn soft_delete_task(
        &self,
        task_id: &TaskId,
        deleted_at: i64,
    ) -> Result<bool, TaskRepositoryError>;
}

/// Supplies new task identifiers for create use cases.
pub trait TaskIdGenerator {
    /// Produces the identifier for a newly created task.
    fn generate_task_id(&self) -> TaskId;
}

/// Captures repository failures that handlers convert into stable application errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskRepositoryError {
    OperationFailed(String),
}
