# 0006 — The plan id hashes outcomes, not presentation

**Status.** Accepted. Recorded here because it lived only in a commit body and
a source comment, and it is named in [GOVERNANCE.md][gov] as the kind of thing
that requires a record.

[gov]: https://github.com/dedalo-org/.github/blob/main/GOVERNANCE.md

## Context

`PayoutPlan::id` is a content hash, prefixed `ded1`. It does three jobs at
once:

- it makes a plan **tamper-evident** — `PayoutPlan::verify` recomputes it and
  refuses a plan whose contents were edited;
- it is the **replay guard on chain** — the claim contract refuses a second
  deposit of the same id;
- it is the **idempotence key in the ledger** — a round already settled is
  refused rather than paid twice.

All three break if the id is not a function of exactly the things that decide
the outcome. Hashing too little forges; hashing too much makes two identical
rounds look different.

## Decision

The id hashes **everything that fixes what is paid, and nothing else**:

| Hashed | Not hashed | Why not |
| --- | --- | --- |
| `project` | `created_at` | Wall-clock time. Two runs over the same history must produce the same id, and a timestamp guarantees they cannot. |
| `asset` (symbol, chain, decimals, contract) | `items[].handle` | A display name. Renaming someone in `dedalo.toml` does not change what any address receives. |
| `range` (branch, from, to) | `items[].score` | Already expressed by `amount`. The score is *why*; the amount is *what*, and only the amount is paid. |
| `split` (gross, protocol, treasury, contributors) | `items[].share_bps` | Derived, and only for human review. |
| `undistributed` | `unresolved[]` | Nobody in it is paid. A wallet appearing later changes a *future* plan, not this one. |
| `items[]` (kind, wallet, amount) | | |

**Every field is absorbed with its byte length in front of it.** Feeding fields
end to end would make the boundaries ambiguous: an asset `US` on chain `DCbase`
and an asset `USDC` on chain `base` produce the same byte stream. A collision
there means a forged plan verifies, or a legitimate round is rejected as
already paid.

**The encoding carries a version tag**, `dedalo.payout-plan.v1`, so any future
change to it is a visible change of id rather than a silent one.

## What was rejected, and why

**Hashing the whole serialised plan.** Simpler, and wrong: it makes the id
depend on `created_at`, on field ordering, and on the serialiser's behaviour.
Two runs over the same history would produce different ids, which destroys the
property the id exists for.

**Excluding `undistributed`.** It is money the round does not move, so it looks
like presentation. It is not: it is determined by who could not be paid, and
two plans that pay the same people but leave different amounts behind are
different rounds. Including it also makes `items` plus `undistributed` equal to
gross a checkable statement about a hashed quantity.

**Including `unresolved`.** Tempting, because it is a real difference between
two plans. But it names people who receive nothing, and its contents change
when an unrelated identity is added to the config. That would make the id move
without any amount moving — which is exactly the failure mode of hashing too
much.

## Consequences

- **`created_at` is informational only**, and is documented as such on the
  field.
- **Renaming a contributor's handle is safe**, and re-running a settled round
  is refused rather than paid twice.
- **A contributor can recompute the id** from a plan they read, without asking
  the maintainer for anything — which is what makes the threat model's "audit
  without permission" claim true.
- **Changing this encoding is breaking**, per [0004](0004-integers-and-basis-points.md)
  and `RELEASING.md`, and requires bumping `ENCODING_VERSION` and superseding
  this record.

## Related

- [0001](0001-pull-not-push.md) — the id is the on-chain replay guard.
- [0004](0004-integers-and-basis-points.md) — why amounts are hashed as
  big-endian integers rather than as their decimal rendering.
