# `cargo-aur` Changelog

## Unreleased

#### Changed

- `cargo aur` will now auto-detect the git host (Github or Gitlab) and generated
  a `source` link based on that.

## 1.0.1 (2020-06-17)

#### Changed

- Use `sha256` instead of `md5`.
- The `install` line in `package()` is now more modern as a one-liner.

## 1.0.0 (2020-06-10)

This is the initial release.
