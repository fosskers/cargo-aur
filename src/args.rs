use std::fs::{DirEntry, File};
use std::io::BufWriter;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::dist::build_package;
use crate::metadata::Config;
use crate::pkgbuild::pkgbuild;
use crate::CargoAurResult;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(next_line_help = true)]
pub struct CargoAurArgs {
    /// Don't actually build anything.
    dryrun: bool,

    #[clap(subcommand)]
    pub(crate) action: CargoAurActions,

    #[clap(short, long, default_value = "target/cargo-aur")]
    pub(crate) output_folder: PathBuf,
}

#[derive(Clone, Debug, Subcommand)]
pub enum CargoAurActions {
    Build {
        /// Use the MUSL build target to produce a static binary.
        #[clap(default_value = "false")]
        musl: bool,
    },
    Generate {
        input: String,
    },
}

pub fn get_args() -> CargoAurArgs {
    CargoAurArgs::parse()
}

impl CargoAurActions {
    pub fn exec(&self, output: &PathBuf, config: &Config, licenses: &[DirEntry]) -> CargoAurResult {
        let generated_file = match self {
            CargoAurActions::Build { musl } => build_package(*musl, output, config, licenses)?,
            CargoAurActions::Generate { input } => input.clone(),
        };

        let mut file = output.clone();
        file.push("PKGBUILD");
        let file = BufWriter::new(File::create(file)?);

        let sha256: String = config.package.sha256sum(generated_file)?;

        pkgbuild(file, config, &sha256, licenses)
    }
}
