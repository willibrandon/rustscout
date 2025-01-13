use std::error::Error;
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum RTraceError {
    Io(io::Error),
    Pattern(String),
    Config(String),
    Search(String),
}

impl fmt::Display for RTraceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RTraceError::Io(err) => write!(f, "IO error: {}", err),
            RTraceError::Pattern(msg) => write!(f, "Pattern error: {}", msg),
            RTraceError::Config(msg) => write!(f, "Configuration error: {}", msg),
            RTraceError::Search(msg) => write!(f, "Search error: {}", msg),
        }
    }
}

impl Error for RTraceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            RTraceError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for RTraceError {
    fn from(err: io::Error) -> Self {
        RTraceError::Io(err)
    }
}
