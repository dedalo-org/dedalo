# Changelog

All notable changes to Dedalo are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Both crates in the workspace share one version and one tag.

<!-- New sections are prepended here by `git cliff` during a release. -->

## Unreleased

### Added

- **core**: the git → attribution → payout plan → settlement pipeline, with the
  first three stages pure and offline so a plan is reproducible from history.
- **core**: integer-only money (`Amount`), largest-remainder splitting that
  conserves every base unit, and a fee schedule in basis points that rounds
  down in the contributors' favour.
- **core**: content-addressed payout plans, an append-only ledger, and refusal
  to settle the same plan twice.
- **cli**: `dedalo init`, `scan`, `contributors`, `plan`, `settle`, `status`,
  `identity` and `ledger`, each with `--json`.
- **infrastructure**: one toolchain pinned in `rust-toolchain.toml` for
  contributors and CI, with the declared MSRV verified rather than asserted.

### Known limitations

- On-chain broadcasting is not live. The `evm` backend validates its
  configuration, re-verifies the plan and builds the distributor call, then
  returns `NotImplemented` rather than signing through an unaudited path.

## 0.0.0 — 2026-08-25

The name, and nothing else.

### Added

- **crate**: a placeholder published to crates.io so that `dedalo` could not be
  taken while the tool was being built. It had no library surface and no
  dependencies; the binary answered `--version` and otherwise printed where the
  code was.

### Notes

- Nothing in it computed, moved or recorded money.
- Publishing an empty version rather than the work in progress was deliberate.
  A version can be yanked but never withdrawn, so the first number that carries
  code should be one somebody can rely on. That number is `0.0.1`.
- It was cut from a branch that no longer exists. For six days `main` held the
  placeholder and the code lived on `v0.1`; the arrangement cost more than it
  bought — `Closes #N` never fired, `cargo install --git` installed a binary
  with no subcommands — and the code came back to `main`.
