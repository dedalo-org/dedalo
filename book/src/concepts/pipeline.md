# The pipeline

Four stages, and the line between the third and the fourth is the most
important line in the codebase.

```text
git  ──▶  attribution  ──▶  payout plan  ──▶  settlement
         ─────────── pure, offline ──────────┤  side effects live here
```

| Stage | Input | Output | May it touch the network? |
| --- | --- | --- | --- |
| [`git`](../concepts/attribution.md) | a repository | merge events | no — reads the working tree |
| [`attribution`](attribution.md) | merge events + policy | integer weights | no |
| [`payout`](plans.md) | weights + fees + identities | a `PayoutPlan` | no |
| [`chain::settlement`](settlement.md) | a plan | a receipt, or a refusal | **yes, and only here** |

## Why the line is there

Stages 1 to 3 are a **pure function**. Given the same repository at the same
commit and the same `dedalo.toml`, they produce byte-identical output on any
machine, in any order, however many times you run them. That is what makes a
plan checkable by someone who does not trust you: they run the same function
and compare ids.

The moment any of those stages could read from the network, that stops being
true. A price feed, a "current" exchange rate, an API that lists contributors —
each one turns a reproducible computation into a snapshot of a moment that
cannot be reproduced. So the rule is absolute rather than a preference:

> **Careful** — if you find yourself reaching for the network inside
> `attribution` or `payout`, the design has gone wrong. There is no exception
> for "just once, cached".

## Stage 1 — read the history

`git::GitBackend` is a trait with four methods: the repository root, the
current branch, resolving a revision, and listing merges matching a query. The
shipped implementation, `CliGit`, drives the `git` binary.

It is a trait for two reasons. Tests substitute a backend built from
`dedalo::testing`, which makes throwaway repositories with real merge commits.
And a different implementation — libgit2, a server-side API, or a version
control system that is not git at all — can be dropped in without touching
anything downstream. The rest of the pipeline never sees a `git` invocation;
it sees `MergeEvent` values.

What a `MergeEvent` carries: the merge commit's hash and date, who pressed
merge, the commits it introduced with their authors and `Co-authored-by:`
trailers, and the aggregated diff of the merge against its first parent.

> **Note** — everything downstream of this stage is already abstract over the
> version control system. The concrete work of making Dedalo run on something
> other than git is [issue #23][vcs]: git stays the reference implementation
> and the source of truth for git projects, but "a merge" is not a git-only
> idea.

## Stage 2 — score it

[Attribution](attribution.md) turns merges into integer weights in
milli-points. Rules come from `[attribution]` in the config; nothing here knows
about money.

## Stage 3 — build the plan

[Payout](plans.md) does three things in a fixed order:

1. Take the [fee schedule](money.md#the-fee-schedule) off the top — protocol
   first, then treasury.
2. Split what remains across contributors by weight, using the
   largest-remainder method.
3. Resolve each contributor to a wallet via
   [identities](identities.md), merging the several emails of one person into
   one item, and listing whoever could not be resolved under `unresolved`.

The result is a `PayoutPlan` and its **id**: a hash over the range, the policy,
the fee schedule and the resulting items. The id deliberately excludes
`created_at`, because the time you ran it is not part of the answer.

## Stage 4 — settle

[Settlement](settlement.md) is the only stage with side effects, and it is
mostly refusals. It re-verifies the plan's id against its contents before doing
anything, refuses the zero address, refuses a round that reaches nobody, and
refuses a plan id the ledger has already settled.

Two backends ship. `dry-run` reports what would move and moves nothing.
`evm` validates the configuration, builds the exact distributor call the plan
translates into, and then returns `Error::NotImplemented` rather than a
receipt — because broadcasting from an unaudited signing path would put real
funds at risk, and a fake receipt would be worse than an honest refusal.

## Where the ledger sits

[The ledger](ledger.md) is not a stage. It is the record the stages write to:
a plan saved with `--save` is recorded, and a settlement appends an entry
naming its parent. It is what makes rounds idempotent — the same plan id
cannot be settled twice — and what `dedalo verify` reads.

[vcs]: https://github.com/dedalo-org/dedalo/issues/23
