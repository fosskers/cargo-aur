mod crates;
mod error;

use crate::error::Error;
use cargo_aur::{GitHost, Package};
use colored::*;
use gumdrop::{Options, ParsingStyle};
use hmac_sha256::Hash;
use serde::Deserialize;
use std::borrow::Cow;
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
    /// Where to obtain the source code for the package.
    /// `crates-io`: Download the package from `crates.io`.
    /// `project`: The package is in the current directory.
    #[options(parse(try_from_str = "parse_source"))]
    source: Option<Source>,
    /// Don't build a binary. Instead, create a PKGBUILD that will build from source.
    no_bin: bool,
    /// Absorbs any extra junk arguments.
    #[options(free)]
    free: Vec<String>,
}

#[derive(Default, Copy, Clone, PartialEq)]
enum Source {
    CratesIo,
    #[default]
    Project,
}

fn parse_source(input: &str) -> Result<Source, &'static str> {
    match input {
        "crates-io" => Ok(Source::CratesIo),
        "project" => Ok(Source::Project),
        _ => Err("Invalid source type, expected `crates-io` or `project`"),
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
    let cargo_target: PathBuf = match std::env::var_os("CARGO_TARGET_DIR") {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from("target"),
    };

    let output = args.output.unwrap_or(cargo_target.join("cargo-aur"));

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

    if args.dryrun.not() {
        let source = args.source.unwrap_or_default();
        let (sha256, license) = match (source, args.no_bin) {
            (Source::CratesIo, true) => {
                let crate_file = crates::CrateFile::download_new(&config)?;
                let sha256 = crate_file.get_sha256sum()?;
                let license = crate_file.get_license()?;
                (sha256, license)
            }
            (Source::CratesIo, false) => {
                let crate_file = crates::CrateFile::download_new(&config)?;
                let built_crate_file = crate_file.build(args.musl)?;
                let license = built_crate_file.tarball(&cargo_target)?;
                let sha256 = sha256sum(&config.package, &output)?;
                (sha256, license)
            }
            (Source::Project, true) => {
                source_tarball(&cargo_target, &output, &config)?;
                let license = alert_if_must_copy_license(&config.package.license)
                    .then(|| license_file(None))
                    .transpose()?;
                let sha256 = sha256sum(&config.package, &output)?;
                (sha256, license)
            }
            (Source::Project, false) => {
                release_build(args.musl)?;
                let license = alert_if_must_copy_license(&config.package.license)
                    .then(|| license_file(None))
                    .transpose()?;
                tarball(args.musl, &cargo_target, &output, license.as_ref(), &config)?;
                let sha256 = sha256sum(&config.package, &output)?;
                (sha256, license)
            }
        };

        // Write the PKGBUILD.
        let path = output.join("PKGBUILD");
        let file = BufWriter::new(File::create(path)?);
        pkgbuild(
            file,
            &config,
            &sha256,
            license.as_ref(),
            source,
            args.no_bin,
        )?;
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

/// Alert the user if the license must be copied, and then return true
/// if the user was alerted.
fn alert_if_must_copy_license(license: &str) -> bool {
    if must_copy_license(license) {
        p("LICENSE file will be installed manually.".bold().yellow());
        return true;
    }
    false
}

/// If a AUR package's license isn't included in `/usr/share/licenses/common/`,
/// then it must be installed manually by the PKGBUILD. MIT and BSD3 are such
/// missing licenses, and since many Rust crates use them we must make this
/// check.
fn must_copy_license(license: &str) -> bool {
    LICENSES.contains(&license).not()
}

/// The path to the `LICENSE` file.
/// First parameter sets the directory to search for it, or if None, it will
/// utilise the current directory.
fn license_file(change_dir_to: Option<&Path>) -> Result<DirEntry, Error> {
    let path = change_dir_to.unwrap_or(Path::new("."));
    std::fs::read_dir(path)?
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
    source_type: Source,
    no_bin: bool,
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
    let source: Cow<str> = match (source_type, no_bin) {
        (Source::Project, _) | (Source::CratesIo, false) => package
            .git_host()
            .unwrap_or(GitHost::Github)
            .source(&config.package, no_bin).into(),
        (Source::CratesIo, true) => "$pkgname-$pkgver.tar.gz::https://static.crates.io/crates/$pkgname/$pkgname-$pkgver.crate".into(),
    };
    let pkgname: Cow<str> = if no_bin {
        package.name.as_str().into()
    } else {
        format!("{}-bin", package.name).into()
    };

    writeln!(file, "{}", authors)?;
    writeln!(file, "#")?;
    writeln!(
        file,
        "# This PKGBUILD was generated by `cargo aur`: https://crates.io/crates/cargo-aur"
    )?;
    writeln!(file)?;
    writeln!(file, "pkgname={}", pkgname)?;
    writeln!(file, "pkgver={}", package.version)?;
    writeln!(file, "pkgrel=1")?;
    writeln!(file, "pkgdesc=\"{}\"", package.description)?;
    writeln!(file, "url=\"{}\"", package.url())?;
    writeln!(file, "license=(\"{}\")", package.license)?;
    writeln!(file, "arch=(\"x86_64\")")?;
    if !no_bin {
        writeln!(file, "provides=(\"{}\")", package.name)?;
        writeln!(file, "conflicts=(\"{}\")", package.name)?;
    }

    match package.metadata.as_ref() {
        Some(metadata) if metadata.non_empty() => {
            writeln!(file, "{}", metadata)?;
        }
        Some(_) | None => {}
    }

    writeln!(file, "source=(\"{}\")", source)?;
    writeln!(file, "sha256sums=(\"{}\")", sha256)?;
    writeln!(file)?;
    // Include the prepare, build and check steps for non-binary package.
    if no_bin {
        writeln!(file, "prepare() {{")?;
        writeln!(file, "    cd $pkgname-$pkgver")?;
        writeln!(file, "    export RUSTUP_TOOLCHAIN=stable")?;
        writeln!(
            file,
            "    cargo fetch --locked --target \"$(rustc -vV | sed -n 's/host: //p')\""
        )?;
        writeln!(file, "}}")?;
        writeln!(file)?;
        writeln!(file, "build() {{")?;
        writeln!(file, "    cd $pkgname-$pkgver")?;
        writeln!(file, "    export RUSTUP_TOOLCHAIN=stable")?;
        writeln!(file, "    export CARGO_TARGET_DIR=target")?;
        writeln!(file, "    cargo build --frozen --release --all-features")?;
        writeln!(file, "}}")?;
        writeln!(file)?;
        writeln!(file, "check() {{")?;
        writeln!(file, "    cd $pkgname-$pkgver")?;
        writeln!(file, "    export RUSTUP_TOOLCHAIN=stable")?;
        writeln!(file, "    cargo test --frozen --all-features")?;
        writeln!(file, "}}")?;
        writeln!(file)?;
    }
    writeln!(file, "package() {{")?;
    // Install command for binary differs depending on bin/no_bin.
    if no_bin {
        // .crate files built by `cargo publish` contain an inner
        // folder that we need to cd into.
        writeln!(file, "    cd $pkgname-$pkgver")?;
        // When building from source, binary will be in the target/release directory.
        writeln!(
            file,
            "    install -Dm755 target/release/{} -t \"$pkgdir/usr/bin\"",
            config.binary_name()
        )?;
    } else {
        writeln!(
            file,
            "    install -Dm755 {} -t \"$pkgdir/usr/bin\"",
            config.binary_name()
        )?;
    };

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

/// Build a source tarball from the current (project) directory.
/// Utilises `cargo --publish --dry-run` under the hood to do the packaging.
fn source_tarball(cargo_target: &Path, output: &Path, config: &Config) -> Result<(), Error> {
    let args = ["publish", "--dry-run", "--allow-dirty"];
    let status = Command::new("cargo").args(args).status()?;
    if !status.success() {
        return Err(Error::Compressing);
    };
    let pkgname = &config.package.name;
    let pkgver = &config.package.version;
    let crate_file_name = format!("{pkgname}-{pkgver}.crate");
    let crate_location = cargo_target.join("package").join(crate_file_name);
    let new_crate_location = config.package.tarball(output);
    std::fs::rename(crate_location, new_crate_location)?;
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
        // NOTE: -C is required, as license may not be in the current directory,
        // but we want it to end up at the root of the tarball.
        command.arg("-C");
        command.arg(lic.path().with_file_name(""));
        command.arg(lic.file_name());
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
    if !command.status()?.success() {
        return Err(Error::Compressing);
    }

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
