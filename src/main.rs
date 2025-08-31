mod error;

use crate::error::Error;
use cargo_aur::{GitHost, Package};
use cargo_metadata::MetadataCommand;
use colored::*;
use gumdrop::{Options, ParsingStyle};
use hmac_sha256::Hash;
use serde::Deserialize;
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
    /// Set a custom output directory (default: target/).
    output: Option<PathBuf>,
    /// Use the MUSL build target to produce a static binary.
    musl: bool,
    /// Don't actually build anything.
    dryrun: bool,
    /// Absorbs any extra junk arguments.
    #[options(free)]
    free: Vec<String>,
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
struct Binary {
    name: String,
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

    // Where cargo expects to read and write to. By default we want to read the
    // built binary from `target/release` and we want to write our results to
    // `target/cargo-aur`, but these are configurable by the user.
    let metadata = MetadataCommand::new().exec()?;
    let cargo_target: PathBuf = metadata.target_directory.canonicalize()?;

    let output = if let Some(pkgname) = metadata.root_package() {
        args.output
            .unwrap_or(cargo_target.join("cargo-aur").join(&pkgname.name))
    } else {
        args.output.unwrap_or(cargo_target.join("cargo-aur"))
    };

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
        tarball(args.musl, &cargo_target, &output, license.as_ref(), &config)?;
        let sha256: String = sha256sum(&config.package, &output)?;

        // Write the PKGBUILD.
        let path = output.join("PKGBUILD");
        let file = BufWriter::new(File::create(path)?);
        pkgbuild(file, &config, &sha256, license.as_ref())?;
    }

    Ok(())
}

/// Read the `Cargo.toml` for all the fields of concern to this tool.
fn cargo_config() -> Result<Config, Error> {
    // NOTE 2023-11-27 Yes it looks silly to be reading the whole thing into a
    // string here, but the `toml` library doesn't allow deserialization from
    // anything else but a string.
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
    writeln!(file, "url=\"{}\"", package.url())?;
    writeln!(file, "license=(\"{}\")", package.license)?;
    writeln!(file, "arch=(\"x86_64\")")?;
    writeln!(file, "provides=(\"{}\")", package.name)?;
    writeln!(file, "conflicts=(\"{}\")", package.name)?;

    match package.metadata.as_ref() {
        Some(metadata) if metadata.non_empty() => {
            writeln!(file, "{}", metadata)?;
        }
        Some(_) | None => {}
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

    if let Some(aur) = package.metadata.as_ref().and_then(|m| m.aur.as_ref()) {
        for (source, target) in aur.files.iter() {
            if target.has_root().not() {
                return Err(Error::TargetNotAbsolute(target.to_path_buf()));
            } else {
                writeln!(
                    file,
                    "    install -Dm644 \"{}\" \"$pkgdir{}\"",
                    source.display(),
                    target.display()
                )?;
            }
        }

        for custom in aur.custom.iter() {
            writeln!(file, "    {}", custom)?;
        }
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
    cargo_target: &Path,
    output: &Path,
    license: Option<&DirEntry>,
    config: &Config,
) -> Result<(), Error> {
    let release_dir = if musl {
        "x86_64-unknown-linux-musl/release"
    } else {
        "release"
    };

    let binary_name = config.binary_name();
    let binary = cargo_target.join(release_dir).join(binary_name);

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
    if let Some(files) = config
        .package
        .metadata
        .as_ref()
        .and_then(|m| m.aur.as_ref())
        .map(|a| a.files.as_slice())
    {
        for (file, _) in files {
            command.arg(file);
        }
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

fn sha256sum(package: &Package, output: &Path) -> Result<String, Error> {
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
