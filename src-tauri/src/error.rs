pub type Result<T> = std::result::Result<T, Error>;

/// Error kind tag serialized over IPC so the frontend can branch on
/// `e.kind === "not_found"` etc. instead of string-matching messages.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    NotFound,
    InvalidInput,
    Sqlite,
    Io,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("{0}")]
    InvalidInput(String),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Error {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Error::NotFound(_) => ErrorKind::NotFound,
            Error::InvalidInput(_) => ErrorKind::InvalidInput,
            Error::Sqlite(_) => ErrorKind::Sqlite,
            Error::Io(_) => ErrorKind::Io,
        }
    }
}

// Tauri command errors cross the IPC boundary as `{ kind, message }` objects.
// The frontend can branch on `kind` for typed handling and fall back to
// `message` for display. See `src/lib/formatError.ts`.
impl serde::Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("Error", 2)?;
        s.serialize_field("kind", &self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}
