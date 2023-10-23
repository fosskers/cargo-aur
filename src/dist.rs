use std::ffi::OsString;
use std::fs::DirEntry;
use std::path::{Path, PathBuf};
use std::process::Command;

use colored::Colorize;

use crate::error::Error;
use crate::metadata::Config;
use crate::{p, CargoAurResult};

pub fn build_package(
    musl: bool,
    output: &PathBuf,
    config: &Config,
    licenses: &[DirEntry],
) -> Result<String, Error> {
    if musl {
        p("Checking for musl toolchain...".bold());
        musl_check()?
    }

    release_build(musl)?;
    tarball(musl, output, licenses, &config)
}

/// Does the user have the `x86_64-unknown-linux-musl` target installed?
fn musl_check() -> CargoAurResult {
    let args = ["target", "list", "--installed"];
    let output = Command::new("rustup").args(args).output()?.stdout;

    std::str::from_utf8(&output)?
        .lines()
        .any(|tc| tc == "x86_64-unknown-linux-musl")
        .then_some(())
        .ok_or(Error::MissingMuslTarget)
}

pub fn tarball(
    musl: bool,
    output: &PathBuf,
    licenses: &[DirEntry],
    config: &Config,
) -> Result<String, Error> {
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
    let mut out_tar = output.clone();
    out_tar.push(config.package.tarball());
    let out_tar = out_tar.to_str().unwrap_or_default().to_string();

    let licenses = licenses.iter().map(|l| l.path()).collect::<Vec<_>>();

    let mut command = Command::new("tar");
    command
        .arg("czf")
        .arg(out_tar.clone())
        .arg(binary_name)
        .args(licenses);
    command.status()?;

    std::fs::remove_file(binary_name)?;

    Ok(out_tar)
}

/// Run `cargo build --release`, potentially building statically.
pub fn release_build(musl: bool) -> CargoAurResult {
    let mut args = vec!["build", "--release"];

    if musl {
        args.push("--target=x86_64-unknown-linux-musl");
    }

    p("Running release build...".bold());
    Command::new("cargo").args(args).status()?;
    Ok(())
}

/// Strip the release binary, so that we aren't compressing more bytes than we
/// need to.
fn strip(path: &Path) -> Result<(), Error> {
    p("Stripping binary...".bold());
    Command::new("strip").arg(path).status()?;
    Ok(()) // FIXME Would love to use my `void` package here and elsewhere.
}
