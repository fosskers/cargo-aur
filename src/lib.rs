//! Independently testable types and functions.

use serde::Deserialize;
use std::ops::Not;
use std::path::{Path, PathBuf};

/// The git forge in which a project's source code is stored.
pub enum GitHost {
    Github,
    Gitlab,
}

impl GitHost {
    pub fn source(&self, package: &Package, no_bin: bool) -> String {
        // Expecting binary tarballs to be uploaded with a platform identifier.
        let platform_identifier = if no_bin { "" } else { "-x86_64" };
        match self {
            GitHost::Github => format!(
                "{}/releases/download/v$pkgver/{}-$pkgver{}.tar.gz",
                package.repository, package.name, platform_identifier
            ),
            GitHost::Gitlab => format!(
                "{}/-/archive/v$pkgver/{}-$pkgver{}.tar.gz",
                package.repository, package.name, platform_identifier
            ),
        }
    }
}

/// The critical fields read from a `Cargo.toml` and rewritten into a PKGBUILD.
#[derive(Deserialize, Debug)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    pub description: String,
    pub repository: String,
    pub license: String,
    pub metadata: Option<Metadata>,
    pub homepage: Option<String>,
    pub documentation: Option<String>,
}

impl Package {
    /// The name of the binary tarball that should be produced from this `Package`.
    pub fn tarball(&self, output: &Path) -> PathBuf {
        output.join(format!("{}-{}-x86_64.tar.gz", self.name, self.version))
    }
    /// The name of the source tarball that should be produced from this `Package`.
    pub fn source_tarball(&self, output: &Path) -> PathBuf {
        output.join(format!("{}-{}.tar.gz", self.name, self.version))
    }

    pub fn git_host(&self) -> Option<GitHost> {
        if self.repository.starts_with("https://github") {
            Some(GitHost::Github)
        } else if self.repository.starts_with("https://gitlab") {
            Some(GitHost::Gitlab)
        } else {
            None
        }
    }

    /// Fetch the package URL from its `homepage`, `documentation` or
    /// `repository` field.
    pub fn url(&self) -> &str {
        self.homepage
            .as_deref()
            .or(self.documentation.as_deref())
            .unwrap_or(&self.repository)
    }
}

// {
//     Package {
//         name: "aura".to_string(),
//         version: "1.2.3".to_string(),
//         authors: vec![],
//         description: "".to_string(),
//         homepage: "".to_string(),
//         repository: "".to_string(),
//         license: "".to_string(),
//         metadata: None,
//     }.tarball(Path::new("foobar"))
// }

/// The `[package.metadata]` TOML block.
#[derive(Deserialize, Debug)]
pub struct Metadata {
    /// Deprecated.
    #[serde(default)]
    pub depends: Vec<String>,
    /// Deprecated.
    #[serde(default)]
    pub optdepends: Vec<String>,
    /// > [package.metadata.aur]
    pub aur: Option<AUR>,
}

impl Metadata {
    /// The metadata block actually has some contents.
    pub fn non_empty(&self) -> bool {
        self.depends.is_empty().not()
            || self.optdepends.is_empty().not()
            || self
                .aur
                .as_ref()
                .is_some_and(|aur| aur.depends.is_empty().not() || aur.optdepends.is_empty().not())
    }
}

impl std::fmt::Display for Metadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Reconcile which section to read extra dependency information from.
        // The format we hope the user is using is:
        //
        // > [package.metadata.aur]
        //
        // But version 1.5 originally supported:
        //
        // > [package.metadata]
        //
        // To avoid a sudden breakage for users, we support both definition
        // locations but favour the newer one.
        //
        // We print a warning to the user elsewhere if they're still using the
        // old way.
        let (deps, opts) = if let Some(aur) = self.aur.as_ref() {
            (aur.depends.as_slice(), aur.optdepends.as_slice())
        } else {
            (self.depends.as_slice(), self.optdepends.as_slice())
        };

        match deps {
            [middle @ .., last] => {
                write!(f, "depends=(")?;
                for item in middle {
                    write!(f, "\"{}\" ", item)?;
                }
                if opts.is_empty().not() {
                    writeln!(f, "\"{}\")", last)?;
                } else {
                    write!(f, "\"{}\")", last)?;
                }
            }
            [] => {}
        }

        match opts {
            [middle @ .., last] => {
                write!(f, "optdepends=(")?;
                for item in middle {
                    write!(f, "\"{}\" ", item)?;
                }
                write!(f, "\"{}\")", last)?;
            }
            [] => {}
        }

        Ok(())
    }
}

/// The inner values of a `[package.metadata.aur]` TOML block.
#[derive(Deserialize, Debug)]
pub struct AUR {
    #[serde(default)]
    depends: Vec<String>,
    #[serde(default)]
    optdepends: Vec<String>,
    #[serde(default)]
    pub files: Vec<(PathBuf, PathBuf)>,
    #[serde(default)]
    pub custom: Vec<String>,
}
