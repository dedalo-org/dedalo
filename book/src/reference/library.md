# Using the library

The binary is a thin shell over the library in the same crate. Everything the
CLI does is available to a bot, a GitHub App, a dashboard, or your own
settlement backend.

**Signatures and types live on [docs.rs/dedalo](https://docs.rs/dedalo)**, which
publishes the reference for each released version. This chapter is the shape of
the thing, not the shape of every function.

## Depending on it

```toml
[dependencies]
dedalo = { version = "0.0.1", default-features = false }
```

`default-features = false` drops the `cli` feature, and with it `clap`,
`tokio`, `toml_edit`, `tracing-subscriber` and `libc`. What is left is the
pipeline.

| Feature | Default | Brings |
| --- | --- | --- |
| `cli` | **on** | The command-line interface and the runtime it needs. |
| `testing` | off | `dedalo::testing`, for building throwaway repositories with real merges. |

Everything under `dedalo::cli` is private except `Cli`, `Command` and the entry
points. Terminal output is not API.

## The short path

`Engine` ties a repository, its config and its ledger together, and is the
shortest route through all four stages:

```rust,ignore
use dedalo::{Engine, money::Amount};

let engine = Engine::discover(".")?;
let merges = engine.scan(None)?;                 // unpaid merges
let attribution = engine.attribute(&merges);     // contribution weights
let plan = engine.plan(&merges, &attribution, Amount::from_base_units(1_000_000))?;

for item in plan.contributors() {
    println!("{:>12} → {}", plan.asset.format_amount(item.amount), item.handle);
}
```

`Engine::discover` walks up from a path looking for `dedalo.toml`, the way git
finds `.git`. `Engine::new` assembles one from parts, for tests or for an
alternative git backend.

## The modules

| Module | Responsibility |
| --- | --- |
| `git` | `GitBackend` trait and `CliGit`, which drives the `git` binary. |
| `attribution` | Merge history → integer contribution weights. |
| `attribution::identity` | Git emails → payable wallets. |
| `money` | `Amount`, `Asset`, and exact splitting. |
| `money::treasury` | The fee schedule and the protocol/treasury/contributor split. |
| `payout` | `PayoutPlan`, its content hash, and its invariants. |
| `chain::wallet` | Validated, checksummed addresses. |
| `chain::merkle` | The claim tree a round is deposited against. |
| `chain::vault` | The rules a deployed contract enforces, as pure functions. |
| `chain::settlement` | The `Settlement` trait, and the dry-run and EVM backends. |
| `storage::ledger` | The hash-chained event log and the payout cursor. |
| `storage::objects` | The content-addressed object store. |
| `config` | `dedalo.toml`, parsed and validated. |
| `error` | `Error` and `Result`. |

Each is usable on its own. `money` has no idea git exists; `attribution` has no
idea money does.

## Substituting a backend

Two traits are meant to be implemented from outside.

### `GitBackend`

Four methods: the repository root, the current branch, resolving a revision,
and listing merges matching a query.

```rust,ignore
use dedalo::git::{GitBackend, HistoryQuery, MergeEvent};

struct MyBackend { /* … */ }

impl GitBackend for MyBackend {
    fn root(&self) -> &std::path::Path { /* … */ }
    fn current_branch(&self) -> dedalo::Result<String> { /* … */ }
    fn resolve(&self, rev: &str) -> dedalo::Result<String> { /* … */ }
    fn merges(&self, query: &HistoryQuery) -> dedalo::Result<Vec<MergeEvent>> { /* … */ }
}
```

Implement it to read from libgit2, from a forge's API, or from a version
control system that is not git. Everything downstream sees `MergeEvent` values
and never knows the difference — which is the groundwork for
[running on more than git][vcs].

### `Settlement`

Implement it to add a chain, or to route a plan through your own custody
process. The contract is narrow on purpose: a settlement re-verifies the plan
before acting, and returns a receipt or an error.

> **Careful** — if you implement this, do not return a receipt for something
> that did not happen. The shipped `evm` backend returns `NotImplemented`
> rather than a plausible-looking success, and that is the standard to hold.

## Testing against real repositories

The `testing` feature builds throwaway repositories with **real merge commits**,
which is why nothing in this project mocks git:

```toml
[dev-dependencies]
dedalo = { version = "0.0.1", features = ["testing"] }
```

```rust,ignore
use dedalo::testing::TempRepo;

let repo = TempRepo::new("example");
repo.merge_feature("feature-a", ("Ada", "ada@example.com"), 40);
repo.merge_feature("feature-b", ("Bea", "bea@example.com"), 40);
```

A mock would only test the mock. `git log --merges` has enough surface — first
parents, trailers, empty merges, octopus merges — that a fake of it tests a
version of git nobody runs.

## Determinism is your problem too

If you build on the library, the guarantee that makes plans checkable is only
as strong as the code around it. Two rules:

- **Do not introduce I/O into stages 1 to 3.** A price feed, a contributor list
  fetched from an API, anything with a clock — each turns a reproducible
  computation into a snapshot nobody else can reproduce.
- **Do not reformat amounts through floats.** `Amount` is a `u128` of base
  units, and `Asset::format_amount` is for display. A round trip through `f64`
  loses base units above 2^53.

[vcs]: https://github.com/dedalo-org/dedalo/issues/23
