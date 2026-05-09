use crate::task::ports::TaskIdGenerator;
use ora_domain::TaskId;
use uuid::Uuid;

/// Generates task identifiers as random UUID v4 values.
#[derive(Clone, Copy, Debug, Default)]
pub struct UuidTaskIdGenerator;

impl UuidTaskIdGenerator {
    /// Creates a UUID-backed task identifier generator.
    pub fn new() -> Self {
        Self
    }
}

impl TaskIdGenerator for UuidTaskIdGenerator {
    /// Produces a fresh UUID v4 task identifier for create requests.
    fn generate_task_id(&self) -> TaskId {
        TaskId::new(Uuid::new_v4().to_string())
    }
}
