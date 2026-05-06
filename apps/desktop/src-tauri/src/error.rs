use serde::Serialize;
use thiserror::Error;

/// Application-wide error type. Serializes to a typed JSON object so the
/// frontend can pattern-match on `kind`.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid path: {0}")]
    InvalidPath(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("other: {0}")]
    Other(String),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let (kind, message) = match self {
            AppError::InvalidPath(m) => ("InvalidPath", m.clone()),
            AppError::Conflict(m) => ("Conflict", m.clone()),
            AppError::NotFound(m) => ("NotFound", m.clone()),
            AppError::Io(e) => ("Io", e.to_string()),
            AppError::Other(m) => ("Other", m.clone()),
        };
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", kind)?;
        s.serialize_field("message", &message)?;
        s.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
