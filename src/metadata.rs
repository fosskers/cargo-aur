use hmac_sha256::Hash;
use serde::Deserialize;
use srtemplate::SrTemplate;

use crate::error::Error;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub(crate) package: Package,
    #[serde(default)]
    pub(crate) bin: Vec<Binary>,
}

impl Config {
    pub fn new() -> Result<Config, Error> {
        let content = std::fs::read_to_string("Cargo.toml")?;
        let proj: Config = toml::from_str(&content)?;
        Ok(proj)
    }
    /// The name of the compiled binary that should be copied to the tarball.
    pub fn binary_name(&self) -> &str {
        self.bin
            .first()
            .map(|bin| bin.name.as_str())
            .unwrap_or(self.package.name.as_str())
    }
}

#[derive(Deserialize, Debug)]
pub struct Package {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) authors: Vec<String>,
    pub(crate) description: String,
    pub(crate) homepage: String,
    pub(crate) repository: String,
    pub(crate) license: String,
    pub(crate) metadata: Option<Metadata>,
}

#[derive(Deserialize, Debug)]
pub struct AUR {
    // Templating package name
    pub(crate) package_name: Option<String>,
    // Templating package version name
    pub(crate) source_download: Option<String>,
    #[serde(default)]
    pub(crate) depends: Vec<String>,
    #[serde(default)]
    pub(crate) optdepends: Vec<String>,
}

impl Package {
    pub fn fill_template(&self, ctx: &SrTemplate) {
        ctx.add_variable("name", &self.name);
        ctx.add_variable("version", &self.version);
        ctx.add_variable("repository", &self.repository);
    }

    pub fn template_name(&self) -> String {
        if let Some(Metadata { aur, .. }) = &self.metadata {
            if let Some(AUR { package_name, .. }) = aur {
                return package_name.clone().unwrap_or(self.name.clone());
            }
        }
        self.name.clone()
    }

    /// The name of the tarball that should be produced from this `Package`.
    pub fn tarball(&self) -> String {
        format!("{}-{}-x86_64.tar.gz", self.name, self.version)
    }

    pub fn git_host(&self) -> Option<GitHost> {
        if let Some(Metadata { aur, .. }) = &self.metadata {
            if let Some(AUR {
                source_download, ..
            }) = aur
            {
                if let Some(source) = source_download {
                    return Some(GitHost::Custom(source.clone()));
                }
            }
        }
        if self.repository.starts_with("https://github") {
            Some(GitHost::Github)
        } else if self.repository.starts_with("https://gitlab") {
            Some(GitHost::Gitlab)
        } else {
            None
        }
    }

    pub fn sha256sum(&self, tar: String) -> Result<String, Error> {
        let bytes = std::fs::read(tar)?;
        let digest = Hash::hash(&bytes);
        let hex = digest.iter().map(|u| format!("{:02x}", u)).collect();
        Ok(hex)
    }
}

#[derive(Default, Debug)]
pub enum GitHost {
    #[default]
    Github,
    Gitlab,
    Custom(String),
}

impl GitHost {
    pub fn source(&self, ctx: &SrTemplate, package: &Package) -> Result<String, Error> {
        match self {
            GitHost::Github => Ok(format!(
                "{}/releases/download/$pkgver/{}-$pkgver-x86_64.tar.gz",
                package.repository, package.name
            )),
            GitHost::Gitlab => Ok(format!(
                "{}/-/archive/$pkgver/{}-$pkgver-x86_64.tar.gz",
                package.repository, package.name
            )),
            GitHost::Custom(src) => ctx.render(src).map_err(Error::TempateError),
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct Binary {
    pub(crate) name: String,
}

#[derive(Deserialize, Debug)]
pub struct Metadata {
    /// Deprecated.
    #[serde(default)]
    pub(crate) depends: Vec<String>,
    /// Deprecated.
    #[serde(default)]
    pub(crate) optdepends: Vec<String>,
    /// > [package.metadata.aur]
    pub(crate) aur: Option<AUR>,
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
                if !opts.is_empty() {
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
