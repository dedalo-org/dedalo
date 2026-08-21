# Dedalo — guide for coding agents

Dedalo turns git merges into contributor payouts. This file is the shared
briefing for anyone working here with an AI coding assistant. It is committed
on purpose: it should be useful to every contributor, not tuned to one person.

## What this repository is

A Cargo workspace with two crates:

| Path | What it is |
| --- | --- |
| `crates/dedalo-core` | the library: git → attribution → payout plan → settlement |
| `crates/dedalo-cli` | the `dedalo` binary, a thin shell over the library |

The pipeline is four stages, and the boundary between them matters:

```
git  ──▶  attribution  ──▶  payout plan  ──▶  settlement
         ─────────── pure, offline ──────────┤  side effects live here
```

Stages 1–3 are deterministic and do no I/O beyond reading the repository.
Stage 4 is the only place anything leaves the machine. Keep it that way: if
you find yourself reaching for the network inside `attribution` or `payout`,
the design has gone wrong.

## Invariants that must not break

These are the reasons the project can be trusted with money. Every one of them
has tests; if you change code near them, add more.

1. **Money is integers.** `money::Amount` is a count of base units. No `f64`
   ever touches a balance. Percentages are basis points (`u16`), never floats.
2. **Splits conserve the total.** `Amount::split_by_weights` uses the
   largest-remainder method and must sum back to exactly the input. A plan's
   items always sum to its gross amount.
3. **Fees round down; dust goes to contributors.** Never the other way round.
4. **Plans are content-addressed.** `PayoutPlan::id` hashes everything that
   determines the outcome, and deliberately excludes `created_at`. Two runs
   over the same history and config must produce the same id.
5. **One wallet, one transfer.** A contributor with several emails is merged
   into a single item before a plan is finalised.
6. **Nobody is silently dropped.** A contributor with no wallet appears in
   `plan.unresolved` with a reason.
7. **Rounds are idempotent.** The ledger refuses to settle the same plan id
   twice. A retried CI job must not pay twice.
8. **Attribution is integer-scored.** Scores are milli-points (`u128`), so the
   same history yields the same weights on every machine.

## Commands

```bash
nix develop                  # pinned toolchain + cargo-nextest, deny, audit, taplo
cargo test --workspace       # unit tests + end-to-end tests against a real git repo
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all
cargo doc --workspace --no-deps --open

nix flake check              # what CI runs: build + tests, clippy, fmt, MSRV
nix build .#docs             # the API reference the docs workflow publishes
cargo deny check             # licence, advisory and source policy
```

Everything CI does is reproducible locally through `nix flake check`. If a
change passes that, it will pass CI.

## Conventions

- **Public items are documented.** `dedalo-core` sets `#![warn(missing_docs)]`
  and CI builds rustdoc with `-D warnings`, so an undocumented `pub` item
  fails the build. Write what the item is *for*, not what its name already
  says.
- **Comments explain why.** The code says what it does. A comment earns its
  place by explaining a decision, a constraint, or a non-obvious consequence.
  Do not narrate the next line.
- **Errors are typed.** `dedalo-core` returns `error::Error`; the CLI wraps
  with `anyhow` and adds user-facing context. Do not `unwrap()` outside tests.
- **Tests live next to the code.** Unit tests in `mod tests`, cross-cutting
  behaviour in `crates/dedalo-core/tests/`. Test names are sentences:
  `split_conserves_every_base_unit`, not `test_split_2`.
- **100 columns**, `rustfmt.toml` is the authority, stable options only.
- **MSRV is enforced, not documented.** `flake.nix` builds the workspace with
  exactly the compiler `rust-version` names, so raising it is a deliberate
  change. Avoid newer language features — let-chains in particular.

## Releasing

One version and one tag for the whole workspace. Never edit the version by
hand: `scripts/bump-version.sh` is the only thing allowed to change it, and the
**Version** workflow drives it. `CHANGELOG.md` is generated from Conventional
Commit subjects, so a pull request title is a release note. Full policy in
[RELEASING.md](RELEASING.md).

## Things to be careful about

- **Never fabricate on-chain behaviour.** The `evm` backend deliberately
  returns `Error::NotImplemented` instead of pretending to broadcast. Do not
  "fix" that by returning a fake receipt — a settlement path that lies is
  worse than one that is missing.
- **Never read or write signing keys.** Keys come from the environment
  variable named by `settlement.signer_env` and must not be logged, echoed,
  copied into config, or committed.
- **`dedalo.toml` and `.dedalo/` are public records.** They belong in git.
  Do not add them to `.gitignore`.
- **Do not weaken a test to make it pass.** If an amount no longer balances,
  the arithmetic is wrong, not the assertion.
- **Never move a published tag.** If a release is broken, fix forward with a
  new version. Someone may already have downloaded the old one.
- **Commands with side effects run once.** `action.yml` deliberately does not
  re-run `settle` to render nicer output; do not add a second invocation.

## Where to start reading

`crates/dedalo-core/src/lib.rs` documents the pipeline and exposes `Engine`,
which is the shortest path through all four stages. From there:
`money.rs` (the arithmetic), `payout.rs` (the artifact), `treasury.rs` (the
fee split).
