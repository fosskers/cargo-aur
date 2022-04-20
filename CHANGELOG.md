# `cargo-aur` Changelog

## Unreleased

#### Added

- Support for `[[bin]]` sections in `Cargo.toml`, allowing you to specify custom
  binary names separate from the package name. [#13]

[#13]: https://github.com/fosskers/cargo-aur/pull/13

## 1.4.1 (2021-09-06)

#### Fixed

- `cargo aur` now respects `CARGO_TARGET_DIR`. [#6]

[#6]: https://github.com/fosskers/cargo-aur/pull/6

## 1.4.0 (2021-06-07)

#### Added

- The `conflicts` field is now added to the `PKGBUILD`.
- Progress messages in the terminal.
- `LICENSE` detection and installation. If your Rust crate has a license not
  found in `/usr/share/licenses/common/` (like `MIT`), then `cargo aur` will
  copy it into the source tarball and have the PKGBUILD install it. Naturally
  this means you must actually have a `LICENSE` file in your project, or `cargo aur` will complain.

## 1.3.0 (2021-04-05)

#### Changed

- `cargo aur` no longer outputs `options=("strip")`, since this is set by
  default in `/etc/makepkg.conf`.

## 1.2.0 (2020-08-24)

#### Added

- A `--version` flag to display the current version of `cargo-aur`.

## 1.1.2 (2020-08-11)

#### Added

- When using `--musl`, the user is warned if they don't have the
  `x86_64-unknown-linux-musl` target installed.

#### Changed

- Run `strip` on the release binary before `tar`ring it.

## 1.1.1 (2020-08-11)

#### Fixed

- A breaking bug in `1.1.0` which prevented it from working at all.

## 1.1.0 (2020-08-10)

#### Added

- The `--musl` flag to compile the release binary with the MUSL target. In most
  cases, this will result in a fully statically linked binary.

## 1.0.3 (2020-07-18)

#### Changed

- Better release profile which produces smaller binaries.

## 1.0.2 (2020-06-22)

#### Changed

- `cargo aur` will now auto-detect the git host (Github or Gitlab) and generated
  a `source` link based on that.
- Fewer dependencies.

## 1.0.1 (2020-06-17)

#### Changed

- Use `sha256` instead of `md5`.
- The `install` line in `package()` is now more modern as a one-liner.

## 1.0.0 (2020-06-10)

This is the initial release.
