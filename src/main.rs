use auto_from::From;
use serde_derive::Deserialize;
use std::{fmt, fs, io, process};

// What it needs to do:
// 1. Read Cargo.toml and produce PKGBUILD.
// 2. Build a release binary if there isn't one.
// 3. Tar the release binary and copy it to the project root.

#[derive(Deserialize, Debug)]
struct Config {
    package: Package,
}

// TODO See how binary size is affected by taking on a `versions` dep.
#[derive(Deserialize, Debug)]
struct Package {
    name: String,
    version: String,
    authors: Vec<String>,
    description: String,
    homepage: String,
    license: String,
}

#[auto_from]
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

// TODO See how binary size is affected by removing `auto_from`.
// impl From<io::Error> for Error {
//     fn from(error: io::Error) -> Self {
//         Error::Io(error)
//     }
// }

fn main() {
    match cargo_config() {
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1)
        }
        Ok(c) => {
            let pkgbuild = pkgbuild(c.package);
            println!("{}", pkgbuild)
        }
    }
}

fn cargo_config() -> Result<Config, Error> {
    let content = fs::read_to_string("Cargo.toml")?;
    let proj = toml::from_str(&content)?;
    Ok(proj) // TODO Would like to do this in one line with the above.
}

/// Produce a legal PKGBUILD.
fn pkgbuild(package: Package) -> String {
    format!(
        r#"
pkgname={}-bin
pkgver={}
pkgrel=1
pkgdesc="{}"
url="{}"
license=('{}')
arch=('x86_64')
provides=('{}')
options=('strip')
"#,
        package.name,
        package.version,
        package.description,
        package.homepage,
        package.license,
        package.name
    )
}
