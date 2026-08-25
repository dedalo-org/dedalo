# Changelog

All notable changes to Dedalo are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Both crates in the workspace share one version and one tag.

<!-- New sections are prepended here by `git cliff` during a release. -->

## Unreleased

Everything under this heading is on the
[`v0.1`](https://github.com/dedalo-org/dedalo/tree/v0.1) branch, not on `main`.
`main` carries the `0.0.0` placeholder described below, and `0.0.1` will be the
first release that ships any of it.

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

- **crate**: a placeholder published to crates.io so that `dedalo` cannot be
  taken while the tool is being built. It has no library surface and no
  dependencies; the binary answers `--version` and otherwise prints where the
  code is.

### Notes

- Nothing here computes, moves or records money. The pipeline, the vault and
  the ledger are on the `v0.1` branch and reach crates.io as `0.0.1`.
- Publishing an empty version rather than the work in progress is deliberate.
  A version can be yanked but never withdrawn, so the first number that carries
  code should be one someone can rely on.
