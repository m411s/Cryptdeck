use rust_i18n::t;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Errors from a backend. `Display` gives a message for the user, in the
/// current language (see `rust_i18n::set_locale`).
#[derive(Debug)]
pub enum Error {
    NotInstalled(String),
    WrongPassword,
    AlreadyMounted,
    VolumeBusy,
    /// Already-translated message about the user's input.
    InvalidInput(String),
    AuthCancelled,
    UntrustedBinary(std::path::PathBuf),
    /// The system needs a one-time change before volumes can be opened;
    /// [`crate::Backend::setup`] makes it.
    SetupNeeded,
    /// A message from a tool or service, or an already-translated one.
    CommandFailed(String),
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotInstalled(name) => f.write_str(&t!("error.not_installed", name = name)),
            Error::WrongPassword => f.write_str(&t!("error.wrong_password")),
            Error::AlreadyMounted => f.write_str(&t!("error.already_mounted")),
            Error::VolumeBusy => f.write_str(&t!("error.busy")),
            Error::AuthCancelled => f.write_str(&t!("error.auth_cancelled")),
            Error::UntrustedBinary(path) => {
                f.write_str(&t!("error.untrusted_binary", path = path.display()))
            }
            Error::SetupNeeded => f.write_str(&t!("error.setup_needed")),
            Error::InvalidInput(msg) | Error::CommandFailed(msg) => f.write_str(msg),
            Error::Io(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
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
