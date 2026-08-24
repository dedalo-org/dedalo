# Roadmap

What is done, what is next, and what is deliberately not being decided yet.

The authoritative list is the [milestones][milestones] and the [issue
tracker][issues]. This page is the shape of it.

## Done

- [x] Git-derived attribution, with co-author support
- [x] Deterministic, content-addressed payout plans
- [x] Fee schedule with protocol / treasury / contributor split
- [x] Append-only hash-chained ledger, with idempotent rounds
- [x] EIP-55 address validation that reports its own strength
- [x] The vault's rules, as pure Rust with a test per refusal
- [x] The Stylus deployable, inside the 24 KiB limit
- [x] GitHub Action wrapper
- [x] Exhaustive proofs for the fee schedule, basis points, small weight
      vectors, and every tree shape to 64 claims
- [x] The verification manifest gate

## v0.1.0 — first release

Publishing the crate, and being honest in the process about what it does and
does not do.

The pipeline works and is tested end to end. What v0.1.0 adds is not capability
but **availability**: a crate on crates.io, an API reference on docs.rs, this
handbook, and documentation good enough that somebody can decide whether to
trust it without reading the source.

## v0.2.0 — on-chain settlement

The list from [the architecture document][arch], and none of it is optional:

1. A claim contract with the Merkle root, a per-round replay guard keyed on the
   plan id, and an expiry path for unclaimed funds.
2. **An independent audit of it, published.**
3. A Safe, with signers who are not one person.
4. A testnet round settled end to end, from `dedalo plan` to a claim.

Until the first four exist, the honest state of this project is what the code
already says: `Error::NotImplemented`.

**Not decided:** which chain to launch on. The template names Base and real
mainnet USDC — a default that was never chosen deliberately and should be.
Testnet first is the safer starting point. Tracked in [issue #15][chain].

## Beyond

### Attribution that measures more than lines

The largest known gap. Review is contribution, and a merge scores nothing for
the person who caught the bug in it. [Review-weighted attribution][reviews] is
the first step; issue triage and documentation written outside the repository
are harder and not yet designed.

### Version control beyond git

Everything downstream of the `git` module is already abstract over the version
control system: `GitBackend` is a trait, and the rest of the pipeline sees
`MergeEvent` values rather than git invocations. Making that real — running on
Jujutsu, Mercurial, Sapling, or a forge's API without a working tree — is
[issue #23][vcs].

The framing matters: **git-compatible, not git-dependent**. Git stays the
reference implementation and the source of truth for git projects. What changes
is that "a merge" stops being a git-only idea in the type system.

### Squash-merge repositories

A repository that squash-merges without merge commits produces no merge events,
and the failure mode is an empty round rather than an error. [Issue #13][squash].

## What is deliberately not on this list

- **A hosted service.** Dedalo runs in your pipeline and reads your repository.
  A dashboard that holds the data is the thing this project exists not to be.
- **A signing key, ever.** Not as an opt-in, not behind a flag. See
  [why](../operating/multisig.md#why-the-key-is-not-in-ci).
- **A token.** The protocol fee flows to an Open Collective wallet. There is
  nothing to buy.
- **Judging contribution.** Dedalo computes what a config says. Whether the
  config is fair is the project's decision, made in the open, in a file that is
  reviewed.

[milestones]: https://github.com/dedalo-org/dedalo/milestones
[issues]: https://github.com/dedalo-org/dedalo/issues
[arch]: https://github.com/dedalo-org/dedalo/blob/main/docs/settlement-architecture.md
[chain]: https://github.com/dedalo-org/dedalo/issues/15
[reviews]: https://github.com/dedalo-org/dedalo/issues/12
[squash]: https://github.com/dedalo-org/dedalo/issues/13
[vcs]: https://github.com/dedalo-org/dedalo/issues/23
