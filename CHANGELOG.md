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
  `identity`, `propose` and `ledger`, each with `--json`.
- **git**: `[git] lands_as` decides what counts as a landed change — a merge
  commit, or every commit on the branch's first-parent line. A squash-merge
  repository produces no merge commits at all, so the previous behaviour paid
  for nothing on the default setting of most projects.
- **chain**: the chain is Solana. Addresses are base58 over thirty-two bytes,
  Merkle leaves are `sha256` over packed little-endian fields, and instruction
  data is Borsh. `dedalo propose` prints the accounts an instruction takes as
  well as its data.
- **money**: the rules a funded project follows over time — a ladder of funding
  thresholds, a four-way split of token revenue, and a periodic distribution by
  role. Pure arithmetic, wired to no chain.
- **infrastructure**: one toolchain pinned in `rust-toolchain.toml` for
  contributors and CI, with the declared MSRV verified rather than asserted.
  Publishing uses crates.io Trusted Publishing, so there is no registry token
  in this repository's secrets.

### Known limitations

- On-chain broadcasting is not live, and the claim program is **not written**.
  `chain::vault` holds every rule such a program must enforce, and the `solana`
  backend re-verifies a plan and then returns `NotImplemented` rather than
  signing through a path nobody has audited.
- **A Solana address carries no checksum.** Every thirty-two byte value is a
  valid key, so a mistyped address that still decodes is accepted.
  `identity link` says so every time, and refuses an address that is off the
  ed25519 curve — one nobody could ever sign for.

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
