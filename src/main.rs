use auto_from::From;
use gumdrop::{Options, ParsingStyle};
use hmac_sha256::Hash;
use itertools::Itertools;
use serde_derive::Deserialize;
use std::process::{self, Command};
use std::{fmt, fs, io};

/// Prepare Rust projects to be released on the Arch Linux User Repository.
#[derive(Options)]
struct Args {
    /// Print this help text.
    help: bool,

    /// Generate Gitlab source links instead.
    gitlab: bool,
}

enum RepoMode {
    Github,
    Gitlab,
}

impl RepoMode {
    fn source(&self, package: &Package) -> String {
        match self {
            RepoMode::Github => format!(
                "{}/releases/download/v$pkgver/{}-$pkgver-x86_64.tar.gz",
                package.repository, package.name
            ),
            RepoMode::Gitlab => format!(
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
}

#[derive(From)]
enum Error {
    Io(io::Error),
    Parsing(toml::de::Error),
    Utf8(std::string::FromUtf8Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{}", e),
            Error::Parsing(e) => write!(f, "{}", e),
            Error::Utf8(e) => write!(f, "{}", e),
        }
    }
}

fn main() {
    let args = Args::parse_args_or_exit(ParsingStyle::AllOptions);

    let mode = if args.gitlab {
        RepoMode::Gitlab
    } else {
        RepoMode::Github
    };

    if let Err(e) = work(mode) {
        eprintln!("{}", e);
        process::exit(1)
    }
}

fn work(mode: RepoMode) -> Result<(), Error> {
    let config = cargo_config()?;
    release_build()?;
    tarball(&config.package)?;
    let sha256 = sha256sum(&config.package)?;
    let pkgbuild = pkgbuild(mode, &config.package, &sha256);
    fs::write("PKGBUILD", pkgbuild)?;

    Ok(())
}

fn cargo_config() -> Result<Config, Error> {
    let content = fs::read_to_string("Cargo.toml")?;
    let proj = toml::from_str(&content)?;
    Ok(proj) // TODO Would like to do this in one line with the above.
}

/// Produce a legal PKGBUILD.
fn pkgbuild(mode: RepoMode, package: &Package, sha256: &str) -> String {
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
options=("strip")
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
        mode.source(package),
        sha256,
        package.name,
    )
}

/// Run `cargo build --release`.
fn release_build() -> Result<(), Error> {
    Command::new("cargo")
        .arg("build")
        .arg("--release")
        .status()?;
    Ok(())
}

fn tarball(package: &Package) -> Result<(), Error> {
    let binary = format!("target/release/{}", package.name);

    fs::copy(binary, &package.name)?;
    Command::new("tar")
        .arg("czf")
        .arg(package.tarball())
        .arg(&package.name)
        .status()?;
    fs::remove_file(&package.name)?;

    Ok(())
}

fn sha256sum(package: &Package) -> Result<String, Error> {
    let bytes = fs::read(package.tarball())?;
    let digest = Hash::hash(&bytes);
    let hex = digest.iter().map(|u| format!("{:02x}", u)).collect();
    Ok(hex)
}
