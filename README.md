# cargo-aur

[![Build](https://github.com/fosskers/cargo-aur/workflows/Build/badge.svg)][3]
[![](https://img.shields.io/crates/v/cargo-aur.svg)][4]
![AUR version][5]

`cargo-aur` is a new subcommand for `cargo` that produces a release tarball and
PKGBUILD file for a Rust project, so that it can be released on the Arch Linux
User Repository (AUR).

No extra configuration is necessary. As long as your `Cargo.toml` has [the usual
fields][0], a PKGBUILD will be generated with all the necessary sections filled
out.

## Installation

Guess what? `cargo-aur` itself is on the AUR! Install it with an AUR-compatible
package manager like [`aura`][1]:

```
sudo aura -A cargo-aur-bin
```

...or via `cargo`:

```
cargo install cargo-aur
```

## Usage

### Basics

Navigate to a Rust project, and run:

```
cargo aur
```

This will produce a `foobar-1.2.3-x86_64.tar.gz` tarball and a PKGBUILD within
`target/cargo-aur`.

If you wish, you can now run `makepkg` to ensure that your package actually builds.

```
> makepkg
==> Making package: cargo-aur-bin 1.0.0-1 (Wed 10 Jun 2020 08:23:46 PM PDT)
==> Checking runtime dependencies...
==> Checking buildtime dependencies...
... etc ...
==> Finished making: cargo-aur-bin 1.0.0-1 (Wed 10 Jun 2020 08:23:47 PM PDT)
```

Notice that the built package itself is postfixed with `-bin`, which follows the
AUR standard.

At this point, it is up to you to:

1. Create an official `Release` on Github/Gitlab, attaching the original binary
   tarball that `cargo aur` produced.
2. Copy the PKGBUILD to a git repo that tracks releases of your package.
3. Run `makepkg --printsrcinfo > .SRCINFO`.
4. Commit both files and push to the AUR.

Some of these steps may be automated in `cargo aur` at a later date if there is
sufficient demand.

### Custom Binary Names

If you specify a `[[bin]]` section in your `Cargo.toml` and set the `name`
field, this will be used as the binary name to install within the PKGBUILD.

### `depends` and `optdepends`

If your package requires other Arch packages at runtime, you can specify these
within your `Cargo.toml` like this:

```toml
[package.metadata.aur]
# give your package a personalized name
package_name = "super-name-bin"
# for custom source download (any other than github or gitlab )
source_download = "https://myserver.com/path/{{ version }}/linux/x86_64"
# Libraries on which your project depends
depends = ["nachos", "pizza"]
optdepends = ["sushi", "ramen"]
```

And these settings will be copied to your PKGBUILD.

### Static Binaries
> [!NOTE]
> You can use the -h or --help flag to see what options `cargo-aur` has


You can use the `build` subcommand to build the binary by default.
> [!NOTE]
>  Run with `--musl` to produce a release binary that is statically linked via [MUSL][2].

```
Usage: cargo aur build [OPTIONS]

Options:
  -m, --musl
          Use the MUSL build target to produce a static binary
  -h, --help
          Print help
```
Example:
> [!NOTE]
> by passing the -or option you can tell it another file on which it will be 

```
> cargo aur build --musl
> cd target/x86_64-unknown-linux-musl/release/
> ldd <your-binary>
    not a dynamic executable
```

You can also use the `generate` command to use a .tar.gz file you already have ready.
```
Usage: cargo aur generate <INPUT>

Arguments:
  <INPUT>


Options:
  -h, --help
          Print help
```
> [!NOTE]
> by passing the -or option you can tell it another file on which it will be 

```
> cargo aur -o out generate target/cargo-aur/cargo-aur-1.6.0-x86_64.tar.gz
> ls ./out
  PKGBUILD
  cargo-aur-1.6.0-x86_64.tar.gz
```

[0]: https://rust-lang.github.io/api-guidelines/documentation.html#c-metadata 
[1]: https://github.com/fosskers/aura
[2]: https://musl.libc.org/
[3]: https://github.com/fosskers/cargo-aur/actions
[4]: https://crates.io/crates/cargo-aur
[5]: https://img.shields.io/aur/version/cargo-aur-bin
