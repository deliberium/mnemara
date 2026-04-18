use std::error::Error as StdError;
use std::fmt::{Display, Formatter};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidConfig(String),
    InvalidRequest(String),
    Conflict(String),
    Unsupported(String),
    Backend(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(f, "invalid config: {message}"),
            Self::InvalidRequest(message) => write!(f, "invalid request: {message}"),
            Self::Conflict(message) => write!(f, "conflict: {message}"),
            Self::Unsupported(message) => write!(f, "unsupported: {message}"),
            Self::Backend(message) => write!(f, "backend error: {message}"),
        }
    }
}

impl StdError for Error {}
