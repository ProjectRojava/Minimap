use minimap_types::NodeType;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(rusqlite::Error),
    #[error("migration error: {0}")]
    Migration(#[from] rusqlite_migration::Error),
    #[error("{node_type} {id} not found")]
    NotFound { node_type: NodeType, id: Uuid },
    #[error("edge {0} not found")]
    EdgeNotFound(Uuid),
    #[error("{node_type} {id} must be archived before it can be deleted")]
    NotArchived { node_type: NodeType, id: Uuid },
    #[error("{node_type} {id} is already archived")]
    AlreadyArchived { node_type: NodeType, id: Uuid },
    #[error("{node_type} {id} is not archived")]
    NotArchivedYet { node_type: NodeType, id: Uuid },
    #[error("that link already exists")]
    DuplicateEdge,
    #[error("invalid value: {0}")]
    Invalid(String),
    /// A database constraint (foreign key, unique, check) rejected the write.
    #[error("constraint violated: {0}")]
    Constraint(String),
    /// The database is encrypted and the key given doesn't open it (or it isn't a database).
    #[error("that key does not open the database")]
    WrongKey,
    /// A backup is encrypted with a key we were not given.
    #[error("this backup is encrypted with a different key")]
    BackupKeyNeeded,
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        match &e {
            rusqlite::Error::SqliteFailure(f, msg)
                if f.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                StoreError::Constraint(msg.clone().unwrap_or_else(|| e.to_string()))
            }
            _ => StoreError::Sqlite(e),
        }
    }
}

pub type Result<T> = std::result::Result<T, StoreError>;
