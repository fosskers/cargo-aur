use crate::{error::Error, license_file, p, Config};
use colored::Colorize;
use hmac_sha256::Hash;
use std::{
    fmt::Write,
    fs::DirEntry,
    path::{Path, PathBuf},
    process::Command,
};

/// Represents a handle to a .crate file that has been downloaded to disk
/// to a temporary folder. The temporary file and folder will be dropped
/// once this goes out of scope.
/// This retains a reference to the Config it was downloaded from, so it
/// is tied to it's lifetime.
pub struct CrateFile<'a> {
    crate_file_prefix: String,
    crate_file_extension: &'static str,
    tempdir_handle: tempfile::TempDir,
    config: &'a Config,
}

/// Represents a handle to a .crate file that has been downloaded to disk
/// to a temporary folder, extracted, and built in release mode.
/// The temporary file(s) and folder will be dropped once this goes out of scope.
/// This retains a reference to the Config it was built from, so it
/// is tied to it's lifetime.
pub struct BuiltCrate<'a> {
    crate_file_prefix: String,
    tempdir_handle: tempfile::TempDir,
    musl: bool,
    config: &'a Config,
}

impl<'a> CrateFile<'a> {
    /// Download the `.crate` file from `crates.io` to a temporary directory,
    /// and if succesful, return a handle to it.
    pub fn download_new(config: &Config) -> Result<CrateFile, Error> {
        let pkgname = &config.package.name;
        let pkgver = &config.package.version;
        // This is downloaded to a temporary directory, instead of the current
        // directory, to avoid the issue where cargo thinks its in a workspace.
        // https://github.com/rust-lang/cargo/issues/5418
        let tempdir_handle = tempfile::tempdir()?;
        let crate_file_prefix = format!("{pkgname}-{pkgver}");
        let crate_file_extension = "tar.gz";
        let crate_file_path_full = tempdir_handle
            .as_ref()
            .join(&crate_file_prefix)
            .with_extension(crate_file_extension);
        let crate_url =
            format!("https://static.crates.io/crates/{pkgname}/{pkgname}-{pkgver}.crate");
        let success = Command::new("curl")
            .arg("--output")
            .arg(crate_file_path_full)
            .arg(&crate_url)
            .status()?
            .success();
        match success {
            true => Ok(CrateFile {
                tempdir_handle,
                crate_file_prefix,
                crate_file_extension,
                config,
            }),
            false => Err(Error::DownloadingCrate { crate_url }),
        }
    }
    /// Get the sha256sum for the downloaded .crate file.
    // NOTE: Possibly future refactor target is crate::sha256sum
    pub fn get_sha256sum(&self) -> Result<String, Error> {
        let crate_file_path = self
            .tempdir_handle
            .as_ref()
            .join(&self.crate_file_prefix)
            .with_extension(self.crate_file_extension);
        let bytes = std::fs::read(crate_file_path)?;
        let digest = Hash::hash(&bytes);
        let hex = digest.iter().fold(String::new(), |mut output, b| {
            write!(output, "{:02x}", b).expect("Write to a string should not fail");
            output
        });
        Ok(hex)
    }
    /// Check to see if the license needs to be installed.
    /// If so, extract the crate, and get the relative path of the LICENSE file
    /// So that we know what 'install' command to run in the PKGBUILD.
    pub fn get_license(&self) -> Result<Option<DirEntry>, Error> {
        let license = if crate::must_copy_license(&self.config.package.license) {
            p("LICENSE file will be installed manually.".bold().yellow());
            Some(self.license_file()?)
        } else {
            None
        };
        Ok(license)
    }
    /// Extract the crate, and get the relative path of the LICENSE file
    fn license_file(&self) -> Result<DirEntry, Error> {
        let crate_filename =
            PathBuf::from(&self.crate_file_prefix).with_extension(self.crate_file_extension);
        if !Command::new("tar")
            .current_dir(self.tempdir_handle.as_ref())
            .arg("-xvzf")
            .arg(&crate_filename)
            .status()?
            .success()
        {
            return Err(Error::ExtractingCrate { crate_filename });
        };
        crate::license_file(Some(
            self.tempdir_handle
                .as_ref()
                .join(&self.crate_file_prefix)
                .as_ref(),
        ))
    }
    /// Build the downloaded crate, and if successful, return a handle to it.
    pub fn build(self, musl: bool) -> Result<BuiltCrate<'a>, Error> {
        let crate_filename =
            PathBuf::from(&self.crate_file_prefix).with_extension(self.crate_file_extension);
        if !Command::new("tar")
            .current_dir(self.tempdir_handle.as_ref())
            .arg("-xvzf")
            .arg(&crate_filename)
            .status()?
            .success()
        {
            return Err(Error::ExtractingCrate { crate_filename });
        };
        // NOTE: Potential refactor target with `super::release_build`.
        let mut args = vec!["build", "--release"];
        if musl {
            args.push("--target=x86_64-unknown-linux-musl");
        }
        p("Running release build...".bold());
        if !Command::new("cargo")
            .current_dir(self.tempdir_handle.as_ref().join(&self.crate_file_prefix))
            .args(args)
            .status()?
            .success()
        {
            return Err(Error::ExtractingCrate { crate_filename });
        };
        Ok(BuiltCrate {
            config: self.config,
            musl,
            crate_file_prefix: self.crate_file_prefix,
            tempdir_handle: self.tempdir_handle,
        })
    }
}

impl<'a> BuiltCrate<'a> {
    /// Create a tarball of the built crate in the `cargo_target` directory.
    /// Returns a reference to the path of the LICENSE file inside the
    /// tarball - it won't simply be ./LICENSE.
    pub fn tarball(self, cargo_target: &Path, output: &Path) -> Result<Option<DirEntry>, Error> {
        let license = if crate::must_copy_license(&self.config.package.license) {
            p("LICENSE file will be installed manually.".bold().yellow());
            Some(self.license_file()?)
        } else {
            None
        };
        super::tarball(
            self.musl,
            cargo_target,
            output,
            license.as_ref(),
            self.config,
        )?;
        Ok(license)
    }
    /// Get the path to the first file with a name starting with LICENSE in the current directory.
    fn license_file(&self) -> Result<DirEntry, Error> {
        crate::license_file(Some(
            self.tempdir_handle
                .as_ref()
                .join(&self.crate_file_prefix)
                .as_ref(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::CrateFile;
    use crate::Config;
    use cargo_aur::Package;
    use std::path::PathBuf;
    /// Download a simple and also popular crate and check the sha256sums.
    #[test]
    fn test_sha256() {
        let package = Package {
            name: "cargo-aur".into(),
            version: "1.7.1".into(),
            authors: Vec::new(),
            description: String::new(),
            repository: String::new(),
            license: "MIT".into(),
            metadata: None,
            homepage: None,
            documentation: None,
        };
        let cfg = Config {
            package,
            bin: Vec::new(),
        };
        let crate_file = CrateFile::download_new(&cfg).unwrap();
        let crate_sha = crate_file.get_sha256sum().unwrap();
        // Hardcoded expected sha
        let expected_sha = "a475dd0482fb5b2782f7edf1728430fa5c2dabc9298771b60e7e7ae708eab31c";
        assert_eq!(crate_sha, expected_sha);
    }
    /// Test that the package can build correctly.
    #[test]
    fn test_build() {
        let package = Package {
            name: "cargo-aur".into(),
            version: "1.7.1".into(),
            authors: Vec::new(),
            description: String::new(),
            repository: String::new(),
            license: "MIT".into(),
            metadata: None,
            homepage: None,
            documentation: None,
        };
        let cfg = Config {
            package,
            bin: Vec::new(),
        };
        CrateFile::download_new(&cfg)
            .unwrap()
            .build(false)
            .expect("Expected build to succeed");
    }
    /// Test that the package can build and tarball correctly.
    #[test]
    fn test_tarball() {
        let package = Package {
            name: "cargo-aur".into(),
            version: "1.7.1".into(),
            authors: Vec::new(),
            description: String::new(),
            repository: String::new(),
            license: "MIT".into(),
            metadata: None,
            homepage: None,
            documentation: None,
        };
        let cfg = Config {
            package,
            bin: Vec::new(),
        };
        let build = CrateFile::download_new(&cfg).unwrap().build(false).unwrap();
        // Lifted from fn super::work
        let cargo_target: PathBuf = match std::env::var_os("CARGO_TARGET_DIR") {
            Some(p) => PathBuf::from(p),
            None => PathBuf::from("target"),
        };
        let output = cargo_target.join("cargo-aur");
        // Ensure the target can actually be written to. Otherwise the `tar`
        // operation later on will fail.
        std::fs::create_dir_all(&output).unwrap();
        build
            .tarball(&cargo_target, &output)
            .expect("Expected tarball to succeed");
    }
}
