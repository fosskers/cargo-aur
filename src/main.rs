pub(crate) mod error;

use crate::error::Error;
use colored::*;
use gumdrop::{Options, ParsingStyle};
use hmac_sha256::Hash;
use itertools::Itertools;
use serde_derive::Deserialize;
use std::fs;
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str;

/// Licenses avaiable from the Arch Linux `licenses`.
///
/// That package contains other licenses, but I've excluded here those unlikely
/// to be used by Rust crates.
const LICENSES: &[&str] = &[
    "AGPL3", "APACHE", "GPL2", "GPL3", "LGPL2.1", "LGPL3", "MPL", "MPL2",
];

#[derive(Options)]
struct Args {
    /// Display this help message.
    help: bool,

    /// Display the current version of this software.
    version: bool,

    /// Unused.
    #[options(free)]
    args: Vec<String>,

    /// Use the MUSL build target to produce a static binary.
    musl: bool,
}

enum GitHost {
    Github,
    Gitlab,
}

impl GitHost {
    fn source(&self, package: &Package) -> String {
        match self {
            GitHost::Github => format!(
                "{}/releases/download/v$pkgver/{}-$pkgver-x86_64.tar.gz",
                package.repository, package.name
            ),
            GitHost::Gitlab => format!(
                "{}/-/archive/v$pkgver/{}-$pkgver-x86_64.tar.gz",
                package.repository, package.name
            ),
        }
    }
}

#[derive(Deserialize, Debug)]
struct Config {
    package: Package,
}

#[derive(Deserialize, Debug)]
struct Package {
    name: String,
    version: String,
    authors: Vec<String>,
    description: String,
    homepage: String,
    repository: String,
    license: String,
}

impl Package {
    /// The name of the tarball that should be produced from this `Package`.
    fn tarball(&self) -> String {
        format!("{}-{}-x86_64.tar.gz", self.name, self.version)
    }

    fn git_host(&self) -> Option<GitHost> {
        if self.repository.starts_with("https://github") {
            Some(GitHost::Github)
        } else if self.repository.starts_with("https://gitlab") {
            Some(GitHost::Gitlab)
        } else {
            None
        }
    }
}

fn main() {
    let args = Args::parse_args_or_exit(ParsingStyle::AllOptions);

    if args.version {
        let version = env!("CARGO_PKG_VERSION");
        println!("{}", version);
    } else if let Err(e) = work(args) {
        eprintln!("{} {}: {}", "::".bold(), "Error".bold().red(), e);
        std::process::exit(1)
    } else {
        println!("{} {}", "::".bold(), "Done.".bold().green());
    }
}

fn work(args: Args) -> Result<(), Error> {
    // We can't proceed if the user has specified `--musl` but doesn't have the
    // target installed.
    if args.musl {
        p("Checking for musl toolchain...".bold());
        musl_check()?
    }

    let package = cargo_config()?;
    let license = if must_copy_license(&package.license) {
        p("LICENSE file will be installed manually.".bold().yellow());
        Some(license_file()?)
    } else {
        None
    };
    release_build(args.musl)?;
    tarball(args.musl, license.as_deref(), &package)?;
    let sha256 = sha256sum(&package)?;
    let pkgbuild = pkgbuild(&package, &sha256);
    fs::write("PKGBUILD", pkgbuild)?;

    Ok(())
}

fn cargo_config() -> Result<Package, Error> {
    let content = fs::read_to_string("Cargo.toml")?;
    let proj: Config = toml::from_str(&content)?;
    Ok(proj.package)
}

/// If a AUR package's license isn't included in `/usr/share/licenses/common/`,
/// then it must be installed manually by the PKGBUILD. MIT is such a missing
/// license, and since many Rust crates use MIT we must make this check.
fn must_copy_license(license: &str) -> bool {
    LICENSES.contains(&license).not()
}

/// The path to the `LICENSE` file.
fn license_file() -> Result<PathBuf, Error> {
    std::fs::read_dir(".")?
        .filter_map(|entry| entry.ok())
        .find(|entry| {
            entry
                .file_name()
                .to_str()
                .map(|s| s.starts_with("LICENSE"))
                .unwrap_or(false)
        })
        .map(|entry| entry.path())
        .ok_or(Error::MissingLicense)
}

/// Produce a legal PKGBUILD.
fn pkgbuild(package: &Package, sha256: &str) -> String {
    format!(
        r#"{}
pkgname={}-bin
pkgver={}
pkgrel=1
pkgdesc="{}"
url="{}"
license=("{}")
arch=("x86_64")
provides=("{}")
conflicts=("{}")
source=("{}")
sha256sums=("{}")

package() {{
    install -Dm755 {} -t "$pkgdir/usr/bin/"
}}
"#,
        package
            .authors
            .iter()
            .map(|a| format!("# Maintainer: {}", a))
            .join("\n"),
        package.name,
        package.version,
        package.description,
        package.homepage,
        package.license,
        package.name,
        package.name,
        package
            .git_host()
            .unwrap_or(GitHost::Github)
            .source(package),
        sha256,
        package.name,
    )
}

/// Run `cargo build --release`, potentially building statically.
fn release_build(musl: bool) -> Result<(), Error> {
    let mut args = vec!["build", "--release"];

    if musl {
        args.push("--target=x86_64-unknown-linux-musl");
    }

    p("Running release build...".bold());
    Command::new("cargo").args(args).status()?;
    Ok(())
}

fn tarball(musl: bool, license: Option<&Path>, package: &Package) -> Result<(), Error> {
    let binary = if musl {
        format!("target/x86_64-unknown-linux-musl/release/{}", package.name)
    } else {
        format!("target/release/{}", package.name)
    };

    strip(&binary)?;
    fs::copy(binary, &package.name)?;

    // Create the tarball.
    p("Packing tarball...".bold());
    let mut command = Command::new("tar");
    command.arg("czf").arg(package.tarball()).arg(&package.name);
    if let Some(lic) = license {
        command.arg(lic);
    }
    command.status()?;

    fs::remove_file(&package.name)?;

    Ok(())
}

/// Strip the release binary, so that we aren't compressing more bytes than we
/// need to.
fn strip(path: &str) -> Result<(), Error> {
    p("Stripping binary...".bold());
    Command::new("strip").arg(path).status()?;
    Ok(()) // FIXME Would love to use my `void` package here and elsewhere.
}

fn sha256sum(package: &Package) -> Result<String, Error> {
    let bytes = fs::read(package.tarball())?;
    let digest = Hash::hash(&bytes);
    let hex = digest.iter().map(|u| format!("{:02x}", u)).collect();
    Ok(hex)
}

/// Does the user have the `x86_64-unknown-linux-musl` target installed?
fn musl_check() -> Result<(), Error> {
    let args = vec!["target", "list", "--installed"];
    let output = Command::new("rustup").args(args).output()?.stdout;
    let installed = str::from_utf8(&output)?
        .lines()
        .any(|tc| tc == "x86_64-unknown-linux-musl");

    if installed {
        Ok(())
    } else {
        Err(Error::MissingTarget)
    }
}

fn p(msg: ColoredString) {
    println!("{} {}", "::".bold(), msg)
}
