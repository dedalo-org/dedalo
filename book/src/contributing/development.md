# Development

Contributing here is, fittingly, the thing the project is built to reward.

The canonical guide is [CONTRIBUTING.md][contributing] in the repository. This
chapter is the working loop and the parts that surprise people.

## Getting a toolchain

Install [`rustup`](https://rustup.rs). It reads `rust-toolchain.toml` when you
enter the repository, so you get the same compiler CI uses without choosing
one.

The MSRV is **1.90.0**, and it is enforced rather than documented: CI builds
with exactly the compiler `rust-version` names. It was raised to 1.90 when the
ABI encoder became `alloy-sol-types` — the fix for RUSTSEC-2026-0220 in `ruint`
needs 1.90, and the alternative was shipping a known-vulnerable big-integer
library in a payments tool.

That is the strongest reason there is to raise a floor, and it is not the only
kind of reason people try. The [policy][msrv] says which ones count: raising the
MSRV is a minor bump pre-1.0, the floor stays at the current stable minus two,
and a dependency that merely *prefers* a newer compiler is not sufficient on its
own. Three files move together when it changes — `Cargo.toml`,
`rust-toolchain.toml` and the workspace flake — and the gate catches two of
them.

[msrv]: https://github.com/dedalo-org/dedalo/blob/main/RELEASING.md#the-minimum-supported-rust-version

## The loop

```bash
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all
```

Those three catch almost everything CI checks. The rest:

```bash
cargo doc --no-deps --open              # rustdoc, with -D warnings in CI
cargo test --release -- --ignored       # the exhaustive proofs
cargo deny check                        # licence, advisory and source policy
cargo publish --dry-run                 # what crates.io would receive
scripts/check-contract.py               # the deployable still fits its chain
```

## What CI checks that your laptop does not

Build and tests on Linux, macOS **and Windows**; the declared MSRV built with
exactly that compiler; rustdoc with `-D warnings`; coverage; the musl packaging
path; and public-API compatibility with the last release.

## Pull requests

**Titles follow [Conventional Commits][cc]** and are checked automatically.
Pull requests are squash merged, so **the title becomes the changelog entry** —
write it for a reader of the release notes:

```text
feat(cli): add `dedalo identity export`
fix(money): keep dust with contributors when a weight is zero
docs: explain how the protocol fee funds the network
```

**One concern per pull request.** A refactor and a behaviour change in the same
diff are two pull requests.

**Anything that changes what people are paid is breaking** — amounts, plan ids,
the fee split — even when it compiles. Say so with `BREAKING CHANGE:` in the
body. See [Releasing](releasing.md).

## Three gates that catch people out

### Public items must be documented

The crate sets `#![warn(missing_docs)]` and CI builds rustdoc with
`-D warnings`, so an undocumented `pub` item **fails the build**.

Write what the item is *for*, not what its name already says. `/// The wallet.`
on a field called `wallet` passes the linter and helps nobody.

### A new module needs a verification entry

`verification.toml` accounts for every module under `src/`, and
`tests/verification_manifest.rs` fails the build if one is missing. Say how it
is verified, or why it needs none — an exemption with a reason is a fine
answer, and the gate keeps the reason true by refusing to let an exempt module
do arithmetic or build an address.

**Adding arithmetic anywhere changes a recorded count and fails the build until
somebody looks.** That is the gate doing its job, not an obstacle to route
around: update `arithmetic_sites` in the same commit that adds the arithmetic,
and let the reviewer see both.

### Money changes carry tests

Anything touching `money`, `attribution`, `money::treasury` or `payout` needs a
test proving the amounts still balance — including the awkward cases: zero
weights, a single payee, amounts that do not divide.

A new rule about what people are paid belongs in `src/money/proofs.rs`,
`src/payout/proofs.rs` or `tests/adversarial.rs`, not only in a hand-picked
example.

> **Careful** — do not weaken a test to make it pass. If an amount no longer
> balances, the arithmetic is wrong, not the assertion.

## Conventions

- **Comments explain why.** The code says what it does. A comment earns its
  place by explaining a decision, a constraint, or a non-obvious consequence.
  Do not narrate the next line.
- **Errors are typed.** The library returns `error::Error`; the CLI wraps with
  `anyhow` and adds user-facing context. **Do not `unwrap()` outside tests.**
- **Tests live next to the code.** Unit tests in `mod tests`, cross-cutting
  behaviour in `tests/`. Test names are sentences:
  `split_conserves_every_base_unit`, not `test_split_2`.
- **Use `dedalo::testing`**, which builds throwaway repositories with real
  merges, rather than mocking git. A mock would only test the mock.
- **100 columns.** `rustfmt.toml` is the authority, stable options only.

## The layout

Only `lib.rs` and `main.rs` sit at the top of `src/`. Everything else is a
directory, because a loose file there is a concern nobody has decided the shape
of yet.

| Path | What it is |
| --- | --- |
| `src/lib.rs` | The crate root: module list and `Engine`. |
| `src/main.rs` | A three-line shim over `dedalo::cli::main`. |
| `src/money/` | Amounts, assets, the fee schedule — and `proofs.rs`. |
| `src/attribution/` | Scoring merges, and the identities they belong to. |
| `src/payout/` | The content-addressed plan — and `proofs.rs`. |
| `src/chain/` | Wallet, merkle, vault, settlement, and the deployable. |
| `src/storage/` | The object store and the hash-chained ledger. |
| `src/git/` | Reading merge history. |
| `src/cli/` | The command surface, behind the default `cli` feature. |
| `tests/` | The four that must run from outside. |

A `proofs.rs` inside a module is its property and exhaustive suite. It compiles
only under `cfg(test)` and ships in no release.

## Things to be careful about

- **Never fabricate on-chain behaviour.** The `solana` backend returns
  `NotImplemented` instead of pretending to broadcast. Do not "fix" that with a
  fake receipt.
- **The leaf encoding is pinned.** `chain::merkle::the_leaf_encoding_has_not_moved`
  holds a root and a proof against a fixed fixture — it is what a deployed
  vault verifies against. Changing it deliberately is fine; the commit has to
  say why.
- **The vault is thin on purpose.** `chain::vault` holds every rule and is
  pure. `src/chain/contract` is a Solana binding and must stay that way — a
  rule that appears there instead of in `vault` is a rule that cannot be
  tested.
- **The deployable has a hard size limit.** Solana rejects anything over 24 KiB
  compressed; `scripts/check-contract.py` measures it.
- **`docs/settlement-architecture.md` is binding.** If the code disagrees with
  it, one of the two is wrong, and the answer is not to quietly change the code.
- **Never read or write signing keys.** Not logged, not echoed, not copied into
  config, not committed.
- **`dedalo.toml` and `.dedalo/` are public records.** They belong in git. Do
  not add them to `.gitignore`.
- **Never move a published tag.** If a release is broken, fix forward.

## Workflow safety

- **Never interpolate `${{ }}` into a `run:` block.** Pass it through `env:`.
  The Action executes in other people's repositories with their secrets in
  scope; `zizmor` fails the build on this and it is right to.
- **Every third-party action is pinned to a commit**, with the tag as a
  trailing comment. A moving tag can be repointed at new code by whoever owns
  it, and these workflows hold release secrets.
- **Never add a checkout to `triage.yml`.** It runs on `pull_request_target`,
  which means a write token; checking out the pull request's code there would
  hand that token to a fork.
- **Builds that publish artifacts do not restore caches.** A cache a pull
  request could have written must not reach a released binary.

[contributing]: https://github.com/dedalo-org/dedalo/blob/main/CONTRIBUTING.md
[cc]: https://www.conventionalcommits.org
