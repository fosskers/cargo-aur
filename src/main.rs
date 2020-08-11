use gumdrop::{Options, ParsingStyle};
use hmac_sha256::Hash;
use itertools::Itertools;
use serde_derive::Deserialize;
use std::fs;
use std::process::{self, Command};

#[derive(Options)]
struct Args {
    help: bool,

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

    if let Err(e) = work(args) {
        eprintln!("{}", e);
        process::exit(1)
    }
}

fn work(args: Args) -> anyhow::Result<()> {
    let config = cargo_config()?;
    release_build(args.musl)?;
    tarball(args.musl, &config.package)?;
    let sha256 = sha256sum(&config.package)?;
    let pkgbuild = pkgbuild(&config.package, &sha256);
    fs::write("PKGBUILD", pkgbuild)?;

    Ok(())
}

fn cargo_config() -> anyhow::Result<Config> {
    let content = fs::read_to_string("Cargo.toml")?;
    let proj = toml::from_str(&content)?;
    Ok(proj) // TODO Would like to do this in one line with the above.
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
        package
            .git_host()
            .unwrap_or(GitHost::Github)
            .source(package),
        sha256,
        package.name,
    )
}

/// Run `cargo build --release`, potentially building statically.
fn release_build(musl: bool) -> anyhow::Result<()> {
    let mut args = vec!["build", "--release"];

    if musl {
        args.push("--target=x86_64-unknown-linux-musl");
    }

    Command::new("cargo").args(args).status()?;
    Ok(())
}

fn tarball(musl: bool, package: &Package) -> anyhow::Result<()> {
    let binary = if musl {
        format!("target/x86_64-unknown-linux-musl/release/{}", package.name)
    } else {
        format!("target/release/{}", package.name)
    };

    fs::copy(binary, &package.name)?;
    Command::new("tar")
        .arg("czf")
        .arg(package.tarball())
        .arg(&package.name)
        .status()?;
    fs::remove_file(&package.name)?;

    Ok(())
}

fn sha256sum(package: &Package) -> anyhow::Result<String> {
    let bytes = fs::read(package.tarball())?;
    let digest = Hash::hash(&bytes);
    let hex = digest.iter().map(|u| format!("{:02x}", u)).collect();
    Ok(hex)
}
