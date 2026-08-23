# Dedalo

[![CI](https://github.com/dedalo-org/dedalo/actions/workflows/ci.yml/badge.svg)](https://github.com/dedalo-org/dedalo/actions/workflows/ci.yml)
[![Security](https://github.com/dedalo-org/dedalo/actions/workflows/security.yml/badge.svg)](https://github.com/dedalo-org/dedalo/actions/workflows/security.yml)
[![docs](https://img.shields.io/badge/docs-api%20reference-blue)](https://dedalo-org.github.io/dedalo/api/)
[![crates.io](https://img.shields.io/crates/v/dedalo.svg)](https://crates.io/crates/dedalo)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

> Turn code merges into sustainable open-source funding.

Dedalo is autonomous financial infrastructure for open-source workflows. It
connects code merges directly to contributor payouts and project treasuries,
eliminating bureaucracy and payment friction.

Everything is derived from **git**: a payout is not a database record someone
typed in, it is a function of merge history plus a config file that lives in
the repository. Anyone can recompute a round and get the same numbers.

## The vision

- **Merge-to-Earn** — code merged into your main branch automatically earns
  transparent, crypto-native rewards for the people who wrote it.
- **Autonomous treasuries** — a share of every round is retained by the
  project, so its budget grows with its activity.
- **Self-funding network** — a protocol fee on every round flows to the
  project's Open Collective wallet. The network is funded by the same flow it
  enables, not by grants.
- **CI/CD native** — runs inside your pipeline, no third-party dashboard, no
  manual invoices.

## How it works

```
git merges  ──▶  attribution  ──▶  payout plan  ──▶  settlement
(source of      (integer          (content-        (on-chain,
 truth)          weights)          addressed)       or simulated)
```

1. **Scan** — read merge commits on the tracked branch, with the commits each
   one introduced and its diff against the mainline.
2. **Attribute** — score merges into integer weights using rules declared in
   `dedalo.toml`: a flat score per merged PR, per-line scoring, a per-merge
   cap, and `Co-authored-by:` splitting.
3. **Plan** — take the protocol fee and the treasury share off the top, then
   split the rest across contributor wallets by weight. The result is a
   content-addressed `PayoutPlan`, reviewable in a pull request.
4. **Settle** — a backend re-verifies the plan and broadcasts it. Nothing
   moves unless you pass `--execute`.

Stages 1–3 are pure and offline. The same repository and the same
`dedalo.toml` always produce the same plan id, on any machine — so a plan
whose id changed is a plan someone tampered with.

### How this is verified

Not claims — gates. CI runs them on every pull request: build and tests on
Linux, macOS and Windows, clippy with `-D warnings`, rustfmt, the declared
MSRV built with exactly the compiler `rust-version` promises, rustdoc,
coverage, packaging, and public-API compatibility.

The test suite runs in five layers:

| Layer | What it holds down |
| --- | --- |
| unit | the arithmetic, parsing and config, next to the code |
| property (`proptest`) | the money invariants below, over thousands of generated rounds |
| **adversarial** | **what the system must refuse — every test is a way money could be lost** |
| end-to-end | the library against real repositories with real merge commits |
| CLI | exit codes and the `--json` shape that `action.yml` parses |

`tests/adversarial.rs` is the one to read first. It asks whether Dedalo can
be made to compute a *wrong* answer: whether two different
plans can share an id, whether one account spelled two ways can be paid twice,
whether a plan id can steer a filesystem path, whether a mistyped address
survives its checksum. Each test marked `FOUND:` is a regression test for a
defect that was real here, not a hypothetical.

### Design guarantees

- **No floating point in money.** Every amount is an integer count of base
  units. Splits use the largest-remainder method.
- **Every base unit is accounted for.** A plan's transfers plus its
  `undistributed` always equal exactly the amount you funded. Nothing is
  created, and nothing goes missing — money that has no destination is stated,
  not dropped.
- **Fees round down.** Rounding remainders land in the contributor pool, never
  in the protocol's pocket.
- **One wallet, one transfer.** Addresses are compared case-insensitively, so
  the two EIP-55 spellings of one account are one payee, not two.
- **Addresses are validated before they are written down.** A mixed-case
  address is checked against its EIP-55 checksum, which catches essentially
  every single-character typo — before it becomes an irreversible transfer.
- **Unpayable contributors are reported, not hidden.** Someone who earned a
  share but has no wallet on file shows up in the plan's `unresolved` list.
- **Idempotent rounds.** The ledger refuses to settle the same plan twice, and
  holds an exclusive lock while doing it, so neither a retry nor a concurrent
  job can pay twice.
- **Settlement refuses more than it accepts.** It will not send to the zero
  address, will not settle a round that reaches nobody, and will not broadcast
  a plan whose id no longer matches its contents.

## Install

```bash
# script — verifies the published SHA-256 before installing
curl -fsSL https://raw.githubusercontent.com/dedalo-org/dedalo/main/install.sh | sh

cargo install dedalo --locked      # from source
cargo binstall dedalo              # prebuilt, no compile
```

Windows builds are published as `.zip` on the
[releases page](https://github.com/dedalo-org/dedalo/releases).

### In CI

Dedalo ships as a GitHub Action, because a payout belongs in the pipeline that
merged the code:

```yaml
- uses: actions/checkout@v5
  with:
    fetch-depth: 0          # attribution needs the full history

- uses: dedalo-org/dedalo@v0
  with:
    command: plan
    amount: "1000"
```

Set `command: settle` and `execute: true` to broadcast, with the signing key in
the environment variable named by `settlement.signer_env`. It defaults to a
simulation, because the safe thing should be the default.

## Quickstart

```bash
cd my-project
dedalo init --open-collective my-project     # writes a commented dedalo.toml
$EDITOR dedalo.toml                          # set the three [wallets] addresses

dedalo identity link ada 0xAda… --email ada@example.com
dedalo identity missing                      # who still has no wallet?

dedalo scan                                  # merges not yet paid for
dedalo contributors                          # scores for the pending range
dedalo plan --amount 1000                    # price a round, spending nothing
dedalo settle --amount 1000                  # simulate the transfers
dedalo settle --amount 1000 --execute        # broadcast, for real
```

A plan looks like this:

```
Round ded106bd7281  2 merges on main → af3141b5
Gross 1000 USDC

PAYEE            KIND         WALLET           SHARE  AMOUNT
ada              contributor  0xAdA00000000…  41.25%   412.5
bea              contributor  0xBeA00000000…  41.25%   412.5
treasury         treasury     0x22222222222…  15.00%     150
demo-collective  protocol     0x33333333333…   2.50%      25
```

Every command takes `--json` for use in scripts and CI.

## Configuration

`dedalo.toml` lives at the repository root and is meant to be committed — it
*is* the project's funding policy, reviewable like any other change.

```toml
[project]
name = "my-project"
open_collective = "my-project"

[git]
branch = "main"

[attribution]
base_points = 100            # flat score per merged PR
points_per_insertion = 1.0
points_per_deletion = 0.5    # deleting code is work too
max_points_per_merge = 5000  # one vendored dep cannot drain a round
split_with_co_authors = true

[asset]
symbol = "USDC"
decimals = 6
chain = "base"
contract = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913"

[fees]
protocol_bps = 250           # 2.5% → the network's Open Collective
treasury_bps = 1500          # 15%  → this project's reserve
                             # 82.5% → contributors

[wallets]
source = "0x…"               # funds each round is paid from
treasury = "0x…"
open_collective = "0x…"

[settlement]
backend = "dry-run"          # or "evm"
signer_env = "DEDALO_SIGNER_KEY"   # the key itself never goes in this file

[[identities]]
handle = "ada"
wallet = "0x…"
emails = ["ada@example.com"]
```

Dedalo keeps its state in `.dedalo/`: an append-only `ledger.jsonl`, the full
JSON of every plan under `plans/`, and a `state.json` cursor recording how far
history has been paid out.

## Using the library

The binary is a thin shell over the library in the same crate, which is
usable on its own — in a bot, a GitHub App, or your own dashboard. Turning the
default features off leaves the pipeline without the argument parser or the
async runtime the CLI needs:

```toml
[dependencies]
dedalo = { version = "0.1", default-features = false }
```

```rust
use dedalo::{Engine, money::Amount};

let engine = Engine::discover(".")?;
let merges = engine.scan(None)?;                 // unpaid merges
let attribution = engine.attribute(&merges);     // contribution weights
let plan = engine.plan(&merges, &attribution, Amount::from_base_units(1_000_000))?;

for item in plan.contributors() {
    println!("{:>12} → {}", plan.asset.format_amount(item.amount), item.handle);
}
```

Each stage is a standalone module, so you can swap any of them:

| Module | Responsibility |
| --- | --- |
| `git` | `GitBackend` trait + `CliGit`, which drives the `git` binary |
| `attribution` | merge history → integer contribution weights |
| `identity` | git emails → payable wallets |
| `treasury` | fee schedule and the protocol/treasury/contributor split |
| `payout` | `PayoutPlan`, its content hash, and its invariants |
| `settlement` | `Settlement` trait, dry-run and EVM backends |
| `ledger` | append-only event log and the payout cursor |

## How funds will move

Nothing broadcasts yet, and the design for when it does is written down in
[docs/settlement-architecture.md](docs/settlement-architecture.md): a **pull**
model where a round is deposited once against a Merkle root and contributors
claim, funded from a **multisig that automation proposes to but cannot sign
for**, over an address layer that knows about address *formats* rather than
one chain.

## Project status

Early. The pipeline from git history to a verified, reproducible payout plan
is implemented and tested end to end. **On-chain broadcasting is not live
yet**: the `evm` backend validates the configuration and builds the exact
distributor call a plan translates into, then stops before signing — shipping
an unaudited signing path would put real funds at risk. Use the default
`dry-run` backend, which produces identical numbers minus the broadcast.

Roadmap, roughly in order:

- [x] Git-derived attribution with co-author support
- [x] Deterministic, content-addressed payout plans
- [x] Fee schedule with protocol / treasury / contributor split
- [x] Append-only ledger with idempotent rounds
- [ ] Audited distributor contract and EVM broadcasting
- [ ] GitHub Action wrapper
- [ ] Review-weighted attribution (reviewers earn too)

## Development

`rustup` picks up the compiler pinned in `rust-toolchain.toml` as soon as you
enter the repository, so the toolchain is the same one CI uses:

```bash
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all
cargo doc --no-deps --open
cargo run -- --help
```

Building on the library? The `testing` feature gives you throwaway
repositories with real merge history:

```rust
use dedalo::testing::TempRepo;

let repo = TempRepo::new("example");
repo.merge_feature("feature-a", ("Ada", "ada@example.com"), 40);
```

### Infrastructure

| Concern | How |
| --- | --- |
| Pinned toolchain | `rust-toolchain.toml`, picked up by `rustup` on entry |
| CI | `.github/workflows/ci.yml` — fmt, clippy, tests on Linux/macOS/Windows, MSRV, rustdoc, coverage, packaging, public-API compatibility |
| MSRV | verified, not asserted: CI builds with exactly the compiler `rust-version` promises |
| Workflow safety | every third-party action pinned to a commit, with the tag as a trailing comment |
| Artifact integrity | SHA-256 checksums plus signed build provenance (`gh attestation verify`) |
| API docs | `.github/workflows/docs.yml` — rustdoc published to GitHub Pages on every push to `main` |
| Releases | `.github/workflows/release.yml` — tagged builds for five targets, checksums, GitHub release, crates.io |
| Supply chain | `.github/workflows/security.yml` — `cargo-deny` and `cargo-audit`, weekly and on manifest changes |
| Dependencies | Dependabot, grouped weekly PRs for Cargo and Actions |
| Versioning | one version and one tag, bumped by a reviewable release pull request — see [RELEASING.md](RELEASING.md) |
| Changelog | generated from Conventional Commit subjects with `git-cliff`; the release notes and `CHANGELOG.md` are the same text |
| Distribution | install script, `cargo install`, `cargo binstall`, GitHub Action |
| Branch policy | `.github/rulesets/main.json`, importable in Settings → Rules |
| Site | `site/` published to GitHub Pages with the API reference under `/api/` |

Public items must be documented: the crate sets `#![warn(missing_docs)]`
and CI builds rustdoc with `-D warnings`, so the published API reference
cannot drift out of date.

## Contributing

Dedalo is open source and built by the community. If you care about Rust,
developer tooling, and sustainable open-source economics, contributions are
welcome — and, fittingly, they are what the project pays out for.

Start with [CONTRIBUTING.md](CONTRIBUTING.md); `src/lib.rs` documents the
pipeline end to end and is the clearest map of the architecture. The
invariants the code guarantees are stated and enforced in
`tests/properties.rs` and `tests/adversarial.rs`.
Security issues go through [SECURITY.md](SECURITY.md), never a public issue.

## License

MIT. See [LICENSE](LICENSE).
