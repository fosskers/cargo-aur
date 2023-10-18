use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug, Default)]
#[command(author, version, about, long_about = None)]
#[command(next_line_help = true)]
pub struct CargoAurArgs {
    /// Unused.
    #[options(free)]
    args: Vec<String>,
    /// Use the MUSL build target to produce a static binary.
    musl: bool,
    /// Don't actually build anything.
    dryrun: bool,

    #[options(help = "the sufix for package name")]
    pub sufix: Option<String>,
}

pub fn get_args() -> CargoAurArgs {
    CargoAurArgs::parse()
}
