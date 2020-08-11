# `cargo-aur` Changelog

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
