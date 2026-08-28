# 0001 — A round is deposited once; contributors claim

**Status.** Accepted. Extracted from `docs/settlement-architecture.md`, where
it was taken before any contract was written.

## Context

A round ends with a `PayoutPlan`: a list of `(wallet, amount)` pairs that sums
to the contributors' share. Something has to turn that list into money in
people's hands.

The obvious design is **push**: the treasury sends one transfer per
contributor, batched into a single transaction by a distributor contract. It is
what everyone sketches first, and it is what this project sketched first.

## Decision

**Pull.** A round is deposited once, against a Merkle root of `(address,
amount)` derived from the plan. Contributors claim what the proof entitles them
to, and pay their own gas to do it.

```
deposit(planId, merkleRoot, token, total)   once, by the project
claim(planId, index, account, amount, proof) once per contributor
```

`planId` is the content hash `PayoutPlan::compute_id` already produces, and is
the per-round replay guard: a round cannot be deposited twice.

## What was rejected, and why

Push, for three reasons that are properties of the problem rather than of any
particular implementation:

| | Push | Pull |
| --- | --- | --- |
| Transactions per round | one batch, or N | one deposit |
| Gas payer | the project | the claimer |
| Unlinked contributor | blocks, or forfeits | claims whenever they link |
| Wrong address | funds destroyed | funds unclaimed, recoverable |
| Partial failure | possible | not expressible |

- **Push needs a valid address for everyone at the moment of payment.** A
  contributor who has not linked a wallet cannot be paid and their share has
  nowhere to go. Under pull, that share sits in the round until they link.
- **Push costs gas proportional to the number of payees**, whether or not those
  payees ever wanted the money on that chain.
- **Push burns a wrong address.** Validation catches typos; it cannot catch an
  address that is well-formed and simply not yours. Under pull, a wrong address
  leaves the funds unclaimed rather than destroyed.

## Consequences

- **`unresolved` and `undistributed` became states, not losses.** Money with
  nowhere to go turns into money not yet claimed.
- **The Merkle root is computed offline** from the plan, so the same history
  and config yield the same root on any machine. The contract only verifies it;
  it does not decide anything.
- **The leaf encoding is now load-bearing and pinned.**
  `chain::merkle::the_leaf_encoding_has_not_moved` holds a root and a proof
  against a fixed fixture, because a deployed vault verifies proofs against it.
  Changing it invalidates every round already deposited.
- **Item ordering is part of the contract.** A claimer's index is a position in
  the tree derived from the plan's items, so the ordering must be
  deterministic — see `chain::merkle`.
- **A round needs an expiry path.** Funds nobody claims cannot sit forever, so
  `CLAIM_WINDOW` exists (180 days) and only the depositor may sweep.

## Still open

- Whether a contributor may assign a claim to an address other than the one in
  the plan.
- Whether a deposit and root may be corrected before the first claim, for a
  round found to be wrong after deposit.
