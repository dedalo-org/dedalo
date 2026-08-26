# Funding from a multisig

This is the step where money moves, and it is the step Dedalo deliberately
cannot do for you.

The authoritative record of these decisions is
[`docs/settlement-architecture.md`][arch] in the repository. It is **binding**:
it records decisions taken before the contract existed, and if the code
disagrees with it, one of the two is wrong and the answer is not to quietly
change the code. This chapter is the operational view of the same thing.

## The shape

```text
dedalo plan --save  ─▶  a reviewed PayoutPlan, content-addressed
dedalo propose      ─▶  1. approve(claimContract, total)
                        2. deposit(planId, merkleRoot, token, total)
                            ↓
                        a Safe, signed by people who are not one person
                            ↓
                        contributors claim, each paying their own gas
```

## Why pull rather than push

The obvious design loops over payees and sends. The pull model deposits once
against a Merkle root and lets each contributor claim. The difference is not
stylistic:

| | Push | Pull |
| --- | --- | --- |
| Transactions per round | one batch, or N | one deposit |
| Gas payer | the project | the claimer |
| Unlinked contributor | blocks the round, or forfeits | claims whenever they link |
| Wrong address | funds destroyed | funds unclaimed, recoverable |
| Partial failure | possible | not expressible |

The last row is the important one. A batch that fails halfway has paid some
people and not others, and there is no good next move. A deposit either
happened or did not.

It also dissolves the "undistributed" problem rather than patching it: money
for a contributor who has not linked a wallet is not money with nowhere to go,
it is money not yet claimed. That is a state, not a loss.

## Why the key is not in CI

The earlier design read a signing key from an environment variable in a CI job.
That key can drain the source wallet, and **everything with write access to a
workflow can reach it** — which includes Dependabot's pull requests and
anything that lands in `.github/`.

A compromised workflow should cost embarrassment, not the treasury.

This is why the pipeline hardening in this project matters as much as the
arithmetic: `zizmor`, pinned action SHAs, and the ban on interpolating
expressions into `run:` blocks all exist because that is the boundary a key in
CI would have crossed.

The consequence is recorded in the code: `settlement.signer_env` described a
capability Dedalo should not have and was **removed**. Do not reintroduce a
config key that names one.

## What a signer should check

Before approving either transaction, and in this order:

1. **The plan id in `propose` matches the plan that was reviewed.** Not the
   amount — the id. Amounts repeat; ids do not.
2. **The `to` address of transaction 1 is the token contract** named in
   `[asset] contract`, and transaction 2's `to` is the claim contract in
   `[settlement] contract`.
3. **The amount in `approve` equals the amount in `deposit`.** An approval
   larger than the deposit leaves an allowance sitting on the token.
4. **The Merkle root matches the plan.** Recompute it from the reviewed plan
   rather than reading it out of the same output you are checking.
5. **The plan id has not been deposited before.** The contract refuses a repeat
   (`RoundExists`), but a signer who notices first saves a failed transaction.

`dedalo propose` prints the calldata so that this comparison is possible
against a document a person can read, rather than against a tool they have to
trust.

## Chain-agnostic, honestly

The address layer knows about address **formats**, not about one chain.
`wallet::AddressKind` names a format; `Address` carries which one it is, and
comparison follows that format's rules — EVM addresses compare
case-insensitively because EIP-55 puts a checksum in the capitalisation, and a
different chain will have different rules. The config is cross-checked, so an
address that is well-formed for the wrong chain is caught.

It is an enum with one variant, not a trait with one implementation. A trait
would be indirection nobody pays for today; the enum says exactly as much as is
true, and adding a chain is four mechanical edits the compiler points at.

**Which chain to launch on is not decided.** The template names Base and real
mainnet USDC — a default that was never chosen deliberately and should be
before anyone broadcasts. Testnet first is the safer starting point, and that
is tracked in [issue #15][chain].

## Before real funds move

The list, from the architecture document:

1. A claim contract with the Merkle root, a per-round replay guard keyed on the
   plan id, and an expiry path for unclaimed funds.
2. **An independent audit of it, published.**
3. A Safe, with signers who are not one person.
4. A testnet round settled end to end, from `dedalo plan` to a claim.
5. ~~Removal of `settlement.signer_env`.~~ **Done.**

Until the first four, the honest state of this project is what the code already
says: `Error::NotImplemented`.

## What exists today, and what it is worth

The vault's rules are pure Rust, driven over their whole domain by tests rather
than by deploying them somewhere and poking them. The deployable binds them to
Solana and does nothing else.

That is worth something. **It is not an audit.** Nobody outside the repository
has looked at it, it has never held a coin, and the reentrancy, SPL token and
expiry paths have been reasoned about by their author and tested by their
author.

What was given up should be said plainly too: the previous vault was Solidity,
and `solc`'s model checker discharged all ten of its arithmetic conditions with
a solver — a stronger statement than any test. Rust has no equivalent that
terminates on this codebase; Kani was measured and rejected. The rules are now
in one language, tested with the same machinery as the rest of the money path,
and proved by nothing.

Treat it as a specification that happens to compile.

[arch]: https://github.com/dedalo-org/dedalo/blob/main/docs/settlement-architecture.md
[chain]: https://github.com/dedalo-org/dedalo/issues/15
