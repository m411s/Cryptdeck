pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} was not found. Is it installed and on your PATH?")]
    NotInstalled(String),

    #[error("{0}")]
    InvalidInput(String),

    #[error("Administrator authentication was cancelled or denied")]
    AuthCancelled,

    #[error("{0}")]
    CommandFailed(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
