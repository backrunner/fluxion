use thiserror::Error;

pub type Result<T> = std::result::Result<T, FluxionError>;

#[derive(Debug, Error)]
pub enum FluxionError {
    #[error("{kind}: {message}")]
    User {
        kind: FluxionErrorKind,
        message: String,
    },
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FluxionErrorKind {
    Network,
    HttpStatus,
    Unauthorized,
    NotFound,
    RangeNotSupported,
    DiskFull,
    PermissionDenied,
    InvalidConfig,
    Storage,
    Cancelled,
    Unsupported,
    Unknown,
}

impl std::fmt::Display for FluxionErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl FluxionError {
    pub fn new(kind: FluxionErrorKind, message: impl Into<String>) -> Self {
        Self::User {
            kind,
            message: message.into(),
        }
    }

    pub fn cancelled() -> Self {
        Self::new(FluxionErrorKind::Cancelled, "task cancelled")
    }

    pub fn kind(&self) -> FluxionErrorKind {
        match self {
            FluxionError::User { kind, .. } => *kind,
            FluxionError::Other(_) => FluxionErrorKind::Unknown,
        }
    }
}

impl From<std::io::Error> for FluxionError {
    fn from(value: std::io::Error) -> Self {
        let kind = match value.kind() {
            std::io::ErrorKind::PermissionDenied => FluxionErrorKind::PermissionDenied,
            _ => FluxionErrorKind::Unknown,
        };
        Self::new(kind, value.to_string())
    }
}

pub trait ResultExt<T> {
    fn boxed(self) -> Result<T>;
}

impl<T, E> ResultExt<T> for std::result::Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn boxed(self) -> Result<T> {
        self.map_err(|error| FluxionError::Other(Box::new(error)))
    }
}
