use std::path::PathBuf;
use std::{fmt, io};

/// Unified result type for dotling operations.
pub type Result<T> = std::result::Result<T, Error>;

/// All errors that dotling can produce.
///
/// Each variant carries enough context to produce a helpful, user-facing
/// message without the caller needing to guess what went wrong.
#[derive(Debug)]
pub enum Error {
    /// An I/O error with path and operation context.
    Io {
        path: PathBuf,
        operation: &'static str,
        source: io::Error,
    },
    /// A configuration file parsing or validation error.
    Config {
        message: String,
        line: Option<usize>,
    },
    /// A cryptographic operation failed.
    Crypto(String),
    /// A deployment operation failed.
    Deploy {
        entry: String,
        message: String,
    },
    /// A vault operation failed.
    Vault(String),
    /// A user-facing error with a clear message (no internal detail needed).
    User(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                path,
                operation,
                source,
            } => {
                write!(f, "{operation} `{}`: {source}", path.display())
            }
            Self::Config {
                message,
                line: Some(n),
            } => {
                write!(f, "config error (line {n}): {message}")
            }
            Self::Config {
                message,
                line: None,
            } => {
                write!(f, "config error: {message}")
            }
            Self::Crypto(msg) => write!(f, "crypto error: {msg}"),
            Self::Deploy { entry, message } => {
                write!(f, "deploy `{entry}`: {message}")
            }
            Self::Vault(msg) => write!(f, "vault error: {msg}"),
            Self::User(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl Error {
    /// Convenience constructor for I/O errors with context.
    pub fn io(path: impl Into<PathBuf>, operation: &'static str, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            operation,
            source,
        }
    }
}
