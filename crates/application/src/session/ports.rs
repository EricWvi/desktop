use ora_domain::{Session, SessionId};

/// Supplies application-owned persistence operations for session CRUD use cases.
///
/// Implementations are expected to hide storage details such as soft-delete columns
/// while preserving the transport-agnostic behavior required by the handlers.
pub trait SessionRepository {
    /// Persists a newly created session and returns the stored snapshot.
    fn create_session(&self, session: Session) -> Result<Session, SessionRepositoryError>;

    /// Loads one visible session by identifier.
    fn find_session(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<Session>, SessionRepositoryError>;

    /// Lists every visible session in storage order.
    fn list_sessions(&self) -> Result<Vec<Session>, SessionRepositoryError>;

    /// Persists a session replacement produced by the application layer.
    fn update_session(&self, session: Session) -> Result<Session, SessionRepositoryError>;

    /// Marks a session deleted and returns whether a visible session was affected.
    fn soft_delete_session(
        &self,
        session_id: &SessionId,
        deleted_at: i64,
    ) -> Result<bool, SessionRepositoryError>;
}

/// Supplies new session identifiers for create use cases.
pub trait SessionIdGenerator {
    /// Produces the identifier for a newly created session.
    fn generate_session_id(&self) -> SessionId;
}

/// Captures repository failures that handlers convert into stable application errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionRepositoryError {
    OperationFailed(String),
}
