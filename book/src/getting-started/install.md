# Install

Dedalo is one crate that builds one binary. Pick whichever of these fits how
you already install things.

## From crates.io

```bash
cargo install dedalo --locked
```

`--locked` builds against the dependency versions the release was tested with.
Without it Cargo is free to pick newer ones, which is usually fine and
occasionally is not the thing you want from a tool that computes payments.

## Prebuilt binary, no compile

```bash
cargo binstall dedalo
```

[`cargo-binstall`](https://github.com/cargo-bins/cargo-binstall) reads the
metadata in `Cargo.toml`, fetches the release archive for your platform from
the [releases page][releases], and skips the compile.

## Install script

```bash
curl -fsSL https://raw.githubusercontent.com/dedalo-org/dedalo/main/install.sh | sh
```

The script verifies the published SHA-256 before it installs anything.

> **Note** — piping a script from the network into a shell is a thing worth
> being awake for. Read [`install.sh`][install] first if you have not; it is
> short, and it is the same file the checksum covers.

## Platforms

Releases are built for five targets:

| Target | Notes |
| --- | --- |
| `x86_64-unknown-linux-gnu` | |
| `x86_64-unknown-linux-musl` | static, for containers and CI images |
| `aarch64-apple-darwin` | Apple silicon |
| `x86_64-apple-darwin` | Intel macs |
| `x86_64-pc-windows-msvc` | published as `.zip` |

Every archive ships with a SHA-256 checksum and signed build provenance. To
check that a binary came from this repository's release workflow and not from
somewhere else:

```bash
gh attestation verify dedalo --repo dedalo-org/dedalo
```

## In a GitHub workflow

Do not install it by hand. Dedalo ships as an action:

```yaml
- uses: dedalo-org/dedalo@v0
  with:
    command: plan
    amount: "1000"
```

See [In CI](../operating/ci.md) for the whole job, including the
`fetch-depth: 0` that attribution needs.

## As a library

The binary is a thin shell over the library in the same crate. Turning the
default features off leaves the pipeline without `clap`, `tokio` or a tracing
subscriber:

```toml
[dependencies]
dedalo = { version = "0.0.1", default-features = false }
```

See [Using the library](../reference/library.md).

## Building from a clone

```bash
git clone https://github.com/dedalo-org/dedalo
cd dedalo
cargo build --release
```

`rustup` reads `rust-toolchain.toml` on entry, so you get the compiler CI uses
without choosing one. The minimum supported version is **1.90.0**, and it is
verified rather than asserted: CI builds with exactly that compiler on every
pull request.

## Check it worked

```console
$ dedalo --version
dedalo 0.1.0
```

## Completions and the man page

Nothing installs these for you, on purpose. `dedalo` generates them from its
own argument definitions:

```console
$ dedalo completions bash > ~/.local/share/bash-completion/completions/dedalo
$ dedalo man > /usr/local/share/man/man1/dedalo.1
```

`install.sh` prints the right command for your shell and does not run it.
Release archives already contain both, under `completions/` and `man/man1/`.

See [the command reference](../reference/cli.md#dedalo-completions-shell) for
where each shell expects its script.

[releases]: https://github.com/dedalo-org/dedalo/releases
[install]: https://github.com/dedalo-org/dedalo/blob/main/install.sh
