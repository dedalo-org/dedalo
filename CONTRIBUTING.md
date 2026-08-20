# Contributing to Dedalo

Dedalo pays contributors for merged code. Contributing here is, fittingly,
the thing the project is built to reward.

## Getting a development environment

With Nix (recommended — you get the exact toolchain CI uses):

```bash
nix develop            # or `direnv allow` once, if you use direnv
```

Without Nix, install the toolchain pinned in `rust-toolchain.toml`; `rustup`
picks it up automatically when you enter the repository.

## The loop

```bash
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all
```

`nix flake check` runs all of the above the way CI does. If it passes locally,
CI will pass.

## What a good pull request looks like

- **One concern per PR.** A refactor and a behaviour change in the same diff
  are two PRs.
- **Public API changes carry rustdoc.** `dedalo-core` sets
  `#![warn(missing_docs)]` and CI builds docs with `-D warnings`, so an
  undocumented `pub` item is a build failure.
- **Money changes carry tests.** Anything touching `money`, `attribution`,
  `treasury` or `payout` needs a test proving the amounts still balance —
  including the awkward cases: zero weights, a single payee, amounts that do
  not divide evenly.
- **Commit messages say why.** The subject is the change; the body is the
  reason.
- **Co-authors get credit.** Use `Co-authored-by:` trailers — Dedalo reads
  them, and they are what splits a payout between pair partners.

## Where things live

See [CLAUDE.md](CLAUDE.md) for the architecture, the invariants the codebase
guarantees, and the conventions. It is written for AI assistants but it is the
clearest map of the project for humans too.

## Getting paid

Once the project's own funding rounds are live, contributions here earn a
share. Add yourself with:

```bash
dedalo identity link <your-handle> <your-wallet> --email <your-git-email>
```

and include it in your first pull request.

## Reporting security issues

Do not open a public issue. See [SECURITY.md](SECURITY.md).
