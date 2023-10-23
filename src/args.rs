use std::fs::{DirEntry, File};
use std::io::BufWriter;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use srtemplate::SrTemplate;

use crate::dist::build_package;
use crate::metadata::Config;
use crate::pkgbuild::pkgbuild;
use crate::CargoAurResult;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(next_line_help = true)]
pub struct CargoAurArgs {
    /// Don't actually build anything.
    #[clap(short, long)]
    dryrun: bool,

    #[clap(subcommand)]
    pub(crate) action: CargoAurActions,

    #[clap(short, long, default_value = "target/cargo-aur")]
    pub(crate) output_folder: PathBuf,
}

#[derive(Clone, Debug, Subcommand)]
pub enum CargoAurActions {
    #[clap(alias = "b")]
    Build {
        /// Use the MUSL build target to produce a static binary.
        #[clap(long, short, default_value = "false")]
        musl: bool,
    },
    #[clap(alias = "g")]
    Generate { input: PathBuf },
}

pub fn get_args() -> CargoAurArgs {
    CargoAurArgs::parse()
}

impl CargoAurActions {
    pub fn exec(&self, output: &PathBuf, config: &Config, licenses: &[DirEntry]) -> CargoAurResult {
        let generated_file = match self {
            CargoAurActions::Build { musl } => build_package(*musl, output, config, licenses)?,
            CargoAurActions::Generate { input } => {
                let file = output.as_path();
                std::fs::copy(input.as_path(), file.join(input.file_name().unwrap()))?;
                input.to_str().unwrap_or_default().to_string()
            }
        };

        let ctx_template = SrTemplate::default();
        config.package.fill_template(&ctx_template);

        let mut file = output.clone();
        file.push("PKGBUILD");
        let file = BufWriter::new(File::create(file)?);

        let sha256: String = config.package.sha256sum(generated_file)?;

        pkgbuild(ctx_template, file, config, &sha256, licenses)
    }
}
