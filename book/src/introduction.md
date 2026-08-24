# Introduction

Dedalo turns merges that are already in a git repository into a deterministic,
auditable payout plan — and, eventually, into money that reaches the people who
wrote the code.

The premise is narrow on purpose. Dedalo keeps no database of who did what. A
round is a **function** of two things that already live in your repository:

```text
merge history  +  dedalo.toml  ──▶  a payout plan, identified by its own hash
```

Run it twice on the same history and the same config, on any machine, and you
get the same plan with the same id. So a plan whose id changed is a plan
someone tampered with, and anybody — a contributor, an auditor, a funder — can
recompute a round instead of trusting the maintainer who published it.

<div class="status-strip">
  <span>status <b class="off">pre-release</b></span>
  <span>on-chain broadcast <b class="off">not live</b></span>
  <span>signing key <b class="off">none, by design</b></span>
  <span>ledger <b>hash-chained</b></span>
  <span>money <b>integers only</b></span>
  <span>msrv <b>1.90.0</b></span>
  <span>licence <b>MIT</b></span>
</div>

## What this book is

The handbook: how Dedalo works, how to operate it, and what its guarantees
actually mean.

It is **not** the API reference. That is generated from the source by rustdoc
and published per released version on
[docs.rs/dedalo](https://docs.rs/dedalo) — the copy that matches the crate you
installed, rather than one built from whatever `main` looked like this morning.
Every link to a type or function in this book goes there.

| You want | Go to |
| --- | --- |
| To run a round today | [Quickstart](getting-started/quickstart.md) |
| To understand the arithmetic | [Money](concepts/money.md) |
| Every config key | [`dedalo.toml`](reference/configuration.md) |
| Every command and flag | [Command line](reference/cli.md) |
| Signatures and types | [docs.rs/dedalo](https://docs.rs/dedalo) |
| To decide whether to trust it | [What is proved](trust/verification.md) |

## Why merges

Because a merge is the moment a project has already decided that work was
worth having. It is reviewed, it is dated, it names its authors and its
co-authors, and it is signed into a history nobody can quietly rewrite. Every
other candidate — issues closed, hours logged, a maintainer's judgement at the
end of the month — needs somebody to type it in, and anything typed in is
something that can be typed in wrong.

That decision has a cost, and this book states it rather than hiding it: work
that never becomes a merge on the tracked branch earns nothing. Review, triage,
documentation written in an issue thread, the design conversation that saved a
month — none of it scores today. [Review-weighted attribution][reviews] is the
first of those gaps being closed, and the [roadmap](contributing/roadmap.md)
names the rest.

[reviews]: https://github.com/dedalo-org/dedalo/issues/12

## What Dedalo will not do

Being explicit about this is most of the reason the project can be trusted with
money at all.

- **It does not hold a signing key.** Not in CI, not in config, not on a
  maintainer's laptop. `dedalo propose` prints transactions; people execute
  them from a multisig. There is no flag that changes this.
- **It does not pretend to broadcast.** The `evm` backend builds the exact
  call a plan translates into and then returns an error rather than a fake
  receipt. A settlement path that lies is worse than one that is missing.
- **It does not round in its own favour.** Fees round down, always, and the
  remainder stays with contributors.
- **It does not silently drop anyone.** A contributor with no wallet on file
  appears in the plan's `unresolved` list with a reason, and the money is
  accounted for rather than absorbed.

## Where things stand

The pipeline from git history to a verified, reproducible payout plan is
implemented and tested end to end. **On-chain settlement is not live.** The
vault's rules are ordinary Rust with a test per refusal; the deployable that
wraps them is an [Arbitrum Stylus](https://arbitrum.io/stylus) crate, and it is
unaudited and undeployed.

[How funds move](operating/multisig.md) lists what has to exist before anything
real moves. Until then the default backend is `dry-run`, which produces
identical numbers minus the broadcast.
