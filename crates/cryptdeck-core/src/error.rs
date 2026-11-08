pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} was not found. Is it installed and on your PATH?")]
    NotInstalled(String),

    #[error(
        "Wrong password, or this isn't a VeraCrypt volume. If you set a PIM or keyfiles \
         when you created the volume, or it's a hidden volume, set that under Advanced options."
    )]
    WrongPassword,

    #[error("This volume is already unlocked")]
    AlreadyMounted,

    #[error("The volume is in use. Close any files or windows that use it, then try again.")]
    VolumeBusy,

    #[error("{0}")]
    InvalidInput(String),

    #[error("Administrator authentication was cancelled")]
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

/// Recognise common failures (from VeraCrypt or cryptsetup) so the UI can explain them plainly.
pub(crate) fn classify(message: String) -> Error {
    let lower = message.to_lowercase();
    if lower.contains("incorrect") && lower.contains("password") {
        Error::WrongPassword
    } else if lower.contains("already mounted") {
        Error::AlreadyMounted
    } else if lower.contains("busy") {
        Error::VolumeBusy
    } else {
        Error::CommandFailed(message)
    }
}
