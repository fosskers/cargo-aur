use auto_from::From;
use itertools::Itertools;
use serde_derive::Deserialize;
use std::process::{self, Command};
use std::{fmt, fs, io};

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
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{}", e),
            Error::Parsing(e) => write!(f, "{}", e),
        }
    }
}

fn main() {
    if let Err(e) = work() {
        eprintln!("{}", e);
        process::exit(1)
    }
}

fn work() -> Result<(), Error> {
    let config = cargo_config()?;
    release_build()?;
    tarball(&config.package)?;
    let md5 = md5sum(&config.package)?;
    let pkgbuild = pkgbuild(&config.package, &md5);
    fs::write("PKGBUILD", pkgbuild)?;

    Ok(())
}

fn cargo_config() -> Result<Config, Error> {
    let content = fs::read_to_string("Cargo.toml")?;
    let proj = toml::from_str(&content)?;
    Ok(proj) // TODO Would like to do this in one line with the above.
}

/// Produce a legal PKGBUILD.
fn pkgbuild(package: &Package, md5: &str) -> String {
    format!(
        r#"{}
pkgname={}-bin
pkgver={}
pkgrel=1
pkgdesc="{}"
url="{}"
license=('{}')
arch=('x86_64')
provides=('{}')
options=('strip')
source=({}/releases/download/v$pkgver/{}-$pkgver-x86_64.tar.gz)
md5sums=('{}')

package() {{
    mkdir -p "$pkgdir/usr/bin/"
    install -m 755 {} "$pkgdir/usr/bin/"
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
        package.repository,
        package.name,
        md5,
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

fn md5sum(package: &Package) -> Result<String, Error> {
    let bytes = fs::read(package.tarball())?;
    let digest = md5::compute(bytes);
    Ok(format!("{:x}", digest))
}
