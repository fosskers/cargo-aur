//! Errors that can occur in this application.

use std::{fmt::Display, path::PathBuf};

pub(crate) enum Error {
    IO(std::io::Error),
    Toml(toml::de::Error),
    Utf8(std::str::Utf8Error),
    Utf8OsString,
    MissingMuslTarget,
    MissingLicense,
    TargetNotAbsolute(PathBuf),
    DownloadingCrate { crate_url: String },
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::IO(e) => write!(f, "{}", e),
            Error::Toml(e) => write!(f, "{}", e),
            Error::Utf8(e) => write!(f, "{}", e),
            Error::Utf8OsString => write!(f, "The `OsString` was not UTF-8!"),
            Error::MissingMuslTarget => write!(
                f,
                "Missing target! Try: rustup target add x86_64-unknown-linux-musl"
            ),
            Error::MissingLicense => {
                write!(f, "Missing LICENSE file. See https://choosealicense.com/")
            }
            Error::TargetNotAbsolute(p) => {
                write!(f, "Target filepath is not absolute: {}", p.display())
            }
            Error::DownloadingCrate { crate_url: url } => {
                write!(
                    f,
                    "Error downloading crate from {crate_url}. Have you published the latest version?"
                )
            }
        }
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(v: std::str::Utf8Error) -> Self {
        Self::Utf8(v)
    }
}

impl From<toml::de::Error> for Error {
    fn from(v: toml::de::Error) -> Self {
        Self::Toml(v)
    }
}

impl From<std::io::Error> for Error {
    fn from(v: std::io::Error) -> Self {
        Self::IO(v)
    }
}
