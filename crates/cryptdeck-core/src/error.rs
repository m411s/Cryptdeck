pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} was not found. Is it installed and on your PATH?")]
    NotInstalled(String),

    #[error("{0}")]
    InvalidInput(String),

    #[error("Administrator authentication was cancelled or denied")]
    AuthCancelled,

    #[error(
        "Refusing to run {} as administrator: it and its folder must be owned by root \
         and not writable by other users. Install VeraCrypt system-wide, or point \
         CRYPTDECK_VERACRYPT at a root-owned copy.",
        .0.display()
    )]
    UntrustedBinary(std::path::PathBuf),

    #[error("{0}")]
    CommandFailed(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
