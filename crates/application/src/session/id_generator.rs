use crate::session::ports::SessionIdGenerator;
use ora_domain::SessionId;
use uuid::Uuid;

/// Generates session identifiers as random UUID v4 values.
#[derive(Clone, Copy, Debug, Default)]
pub struct UuidSessionIdGenerator;

impl UuidSessionIdGenerator {
    /// Creates a UUID-backed session identifier generator.
    pub fn new() -> Self {
        Self
    }
}

impl SessionIdGenerator for UuidSessionIdGenerator {
    /// Produces a fresh UUID v4 session identifier for create requests.
    fn generate_session_id(&self) -> SessionId {
        SessionId::new(Uuid::new_v4().to_string())
    }
}
