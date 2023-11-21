mod error;

use crate::error::Error;
use colored::*;
use gumdrop::{Options, ParsingStyle};
use hmac_sha256::Hash;
use serde::Deserialize;
use std::ffi::OsString;
use std::fs::{DirEntry, File};
use std::io::{BufWriter, Write};
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Licenses available from the Arch Linux `licenses` package.
///
/// That package contains other licenses, but I've excluded here those unlikely
/// to be used by Rust crates.
const LICENSES: &[&str] = &[
    "AGPL-3.0-only",
    "AGPL-3.0-or-later",
    "Apache-2.0",
    "BSL-1.0", // Boost Software License.
    "GPL-2.0-only",
    "GPL-2.0-or-later",
    "GPL-3.0-only",
    "GPL-3.0-or-later",
    "LGPL-2.0-only",
    "LGPL-2.0-or-later",
    "LGPL-3.0-only",
    "LGPL-3.0-or-later",
    "MPL-2.0",   // Mozilla Public License.
    "Unlicense", // Not to be confused with "Unlicensed".
];

#[derive(Options)]
struct Args {
    /// Display this help message.
    help: bool,
    /// Display the current version of this software.
    version: bool,

    /// Set custom output directory
    output: Option<PathBuf>,
    /// Unused.
    #[options(free)]
    args: Vec<String>,
    /// Use the MUSL build target to produce a static binary.
    musl: bool,
    /// Don't actually build anything.
    dryrun: bool,
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
    #[serde(default)]
    bin: Vec<Binary>,
}

impl Config {
    /// The name of the compiled binary that should be copied to the tarball.
    fn binary_name(&self) -> &str {
        self.bin
            .first()
            .map(|bin| bin.name.as_str())
            .unwrap_or(self.package.name.as_str())
    }
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
    metadata: Option<Metadata>,
}

#[derive(Deserialize, Debug)]
struct Metadata {
    /// Deprecated.
    #[serde(default)]
    depends: Vec<String>,
    /// Deprecated.
    #[serde(default)]
    optdepends: Vec<String>,
    /// > [package.metadata.aur]
    aur: Option<AUR>,
}

#[derive(Deserialize, Debug)]
struct AUR {
    #[serde(default)]
    depends: Vec<String>,
    #[serde(default)]
    optdepends: Vec<String>,
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

#[derive(Deserialize, Debug)]
struct Binary {
    name: String,
}

impl Package {
    /// The name of the tarball that should be produced from this `Package`.
    fn tarball(&self, output: &PathBuf) -> String {
        let mut output = output.clone();
        output.push(format!("{}-{}-x86_64.tar.gz", self.name, self.version));
        output.to_str().unwrap().to_string()
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

fn main() -> ExitCode {
    let args = Args::parse_args_or_exit(ParsingStyle::AllOptions);

    if args.version {
        let version = env!("CARGO_PKG_VERSION");
        println!("{}", version);
        ExitCode::SUCCESS
    } else if let Err(e) = work(args) {
        eprintln!("{} {}: {}", "::".bold(), "Error".bold().red(), e);
        ExitCode::FAILURE
    } else {
        println!("{} {}", "::".bold(), "Done.".bold().green());
        ExitCode::SUCCESS
    }
}

fn work(args: Args) -> Result<(), Error> {
    // We can't proceed if the user has specified `--musl` but doesn't have the
    // target installed.
    if args.musl {
        p("Checking for musl toolchain...".bold());
        musl_check()?
    }

    let output = args.output.unwrap_or(PathBuf::from("target/cargo-aur"));

    // Ensure the target can actually be written to. Otherwise the `tar`
    // operation later on will fail.
    std::fs::create_dir_all(&output)?;

    let config = cargo_config()?;

    // Warn if the user if still using the old metadata definition style.
    if let Some(metadata) = config.package.metadata.as_ref() {
        if metadata.depends.is_empty().not() || metadata.optdepends.is_empty().not() {
            p("Use of [package.metadata] is deprecated. Please specify extra dependencies under [package.metadata.aur].".bold().yellow());
        }
    }

    let license = if must_copy_license(&config.package.license) {
        p("LICENSE file will be installed manually.".bold().yellow());
        Some(license_file()?)
    } else {
        None
    };

    if args.dryrun.not() {
        release_build(args.musl)?;
        tarball(args.musl, &output, license.as_ref(), &config)?;
        let sha256: String = sha256sum(&config.package, &output)?;

        // Write the PKGBUILD.
        {
            let mut output = output.clone();
            output.push("PKGBUILD");
            let file = BufWriter::new(File::create(&output)?);
            pkgbuild(file, &config, &sha256, license.as_ref())?;
        }
    }

    Ok(())
}

fn cargo_config() -> Result<Config, Error> {
    let content = std::fs::read_to_string("Cargo.toml")?;
    let proj: Config = toml::from_str(&content)?;
    Ok(proj)
}

/// If a AUR package's license isn't included in `/usr/share/licenses/common/`,
/// then it must be installed manually by the PKGBUILD. MIT and BSD3 are such
/// missing licenses, and since many Rust crates use them we must make this
/// check.
fn must_copy_license(license: &str) -> bool {
    LICENSES.contains(&license).not()
}

/// The path to the `LICENSE` file.
fn license_file() -> Result<DirEntry, Error> {
    std::fs::read_dir(".")?
        .filter_map(|entry| entry.ok())
        .find(|entry| {
            entry
                .file_name()
                .to_str()
                .map(|s| s.starts_with("LICENSE"))
                .unwrap_or(false)
        })
        .ok_or(Error::MissingLicense)
}

/// Write a legal PKGBUILD to some `Write` instance (a `File` in this case).
fn pkgbuild<T>(
    mut file: T,
    config: &Config,
    sha256: &str,
    license: Option<&DirEntry>,
) -> Result<(), Error>
where
    T: Write,
{
    let package = &config.package;
    let authors = package
        .authors
        .iter()
        .map(|a| format!("# Maintainer: {}", a))
        .collect::<Vec<_>>()
        .join("\n");
    let source = package
        .git_host()
        .unwrap_or(GitHost::Github)
        .source(&config.package);

    writeln!(file, "{}", authors)?;
    writeln!(file, "#")?;
    writeln!(
        file,
        "# This PKGBUILD was generated by `cargo aur`: https://crates.io/crates/cargo-aur"
    )?;
    writeln!(file)?;
    writeln!(file, "pkgname={}-bin", package.name)?;
    writeln!(file, "pkgver={}", package.version)?;
    writeln!(file, "pkgrel=1")?;
    writeln!(file, "pkgdesc=\"{}\"", package.description)?;
    writeln!(file, "url=\"{}\"", package.homepage)?;
    writeln!(file, "license=(\"{}\")", package.license)?;
    writeln!(file, "arch=(\"x86_64\")")?;
    writeln!(file, "provides=(\"{}\")", package.name)?;
    writeln!(file, "conflicts=(\"{}\")", package.name)?;

    if let Some(metadata) = package.metadata.as_ref() {
        writeln!(file, "{}", metadata)?;
    }

    writeln!(file, "source=(\"{}\")", source)?;
    writeln!(file, "sha256sums=(\"{}\")", sha256)?;
    writeln!(file)?;
    writeln!(file, "package() {{")?;
    writeln!(
        file,
        "    install -Dm755 {} -t \"$pkgdir/usr/bin\"",
        config.binary_name()
    )?;

    if let Some(lic) = license {
        let file_name = lic
            .file_name()
            .into_string()
            .map_err(|_| Error::Utf8OsString)?;
        writeln!(
            file,
            "    install -Dm644 {} \"$pkgdir/usr/share/licenses/$pkgname/{}\"",
            file_name, file_name
        )?;
    }

    writeln!(file, "}}")?;
    Ok(())
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

fn tarball(
    musl: bool,
    output: &PathBuf,
    license: Option<&DirEntry>,
    config: &Config,
) -> Result<(), Error> {
    let target_dir: OsString = match std::env::var_os("CARGO_TARGET_DIR") {
        Some(p) => p,
        None => "target".into(),
    };

    let release_dir = if musl {
        "x86_64-unknown-linux-musl/release"
    } else {
        "release"
    };

    let binary_name = config.binary_name();
    let mut binary: PathBuf = target_dir.into();
    binary.push(release_dir);
    binary.push(binary_name);

    strip(&binary)?;
    std::fs::copy(binary, binary_name)?;

    // Create the tarball.
    p("Packing tarball...".bold());
    let mut command = Command::new("tar");
    command
        .arg("czf")
        .arg(config.package.tarball(output))
        .arg(binary_name);
    if let Some(lic) = license {
        command.arg(lic.path());
    }
    command.status()?;

    std::fs::remove_file(binary_name)?;

    Ok(())
}

/// Strip the release binary, so that we aren't compressing more bytes than we
/// need to.
fn strip(path: &Path) -> Result<(), Error> {
    p("Stripping binary...".bold());
    Command::new("strip").arg(path).status()?;
    Ok(()) // FIXME Would love to use my `void` package here and elsewhere.
}

fn sha256sum(package: &Package, output: &PathBuf) -> Result<String, Error> {
    let bytes = std::fs::read(package.tarball(output))?;
    let digest = Hash::hash(&bytes);
    let hex = digest.iter().map(|u| format!("{:02x}", u)).collect();
    Ok(hex)
}

/// Does the user have the `x86_64-unknown-linux-musl` target installed?
fn musl_check() -> Result<(), Error> {
    let args = ["target", "list", "--installed"];
    let output = Command::new("rustup").args(args).output()?.stdout;

    std::str::from_utf8(&output)?
        .lines()
        .any(|tc| tc == "x86_64-unknown-linux-musl")
        .then_some(())
        .ok_or(Error::MissingMuslTarget)
}

fn p(msg: ColoredString) {
    println!("{} {}", "::".bold(), msg)
}
