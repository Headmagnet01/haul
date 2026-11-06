/// Everything that can go wrong inside the engine.
#[derive(thiserror::Error, Debug)]
pub enum EngineError {
    #[error("offline (404)")]
    Offline,
    #[error("link expired ({0})")]
    LinkExpired(u16),
    #[error("http status {0}")]
    Http(u16),
    #[error("range not satisfiable")]
    RangeNotSatisfiable,
    #[error("disk full")]
    DiskFull,
    #[error("network: {0}")]
    Network(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("database: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("cancelled")]
    Cancelled,
    /// Bad input from the user; the text is shown to them as is.
    #[error("{0}")]
    Invalid(String),
}

impl EngineError {
    /// Network hiccups, server errors (5xx) and 429 Too Many Requests are worth retrying.
    pub fn is_retryable(&self) -> bool {
        match self {
            EngineError::Network(_) => true,
            EngineError::Http(code) => *code == 429 || (500..600).contains(code),
            _ => false,
        }
    }

    /// Maps a final HTTP status to an error, following the spec's status table.
    pub fn from_status(code: u16) -> EngineError {
        match code {
            404 => EngineError::Offline,
            403 | 410 => EngineError::LinkExpired(code),
            416 => EngineError::RangeNotSatisfiable,
            _ => EngineError::Http(code),
        }
    }
}

impl From<reqwest::Error> for EngineError {
    fn from(e: reqwest::Error) -> Self {
        match e.status() {
            Some(s) => EngineError::from_status(s.as_u16()),
            None => EngineError::Network(e.to_string()),
        }
    }
}
