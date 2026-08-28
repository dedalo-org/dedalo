# 0004 — Money is integers; percentages are basis points

**Status.** Accepted. This is the oldest decision in the project and the last
one to be written down, which is the usual fate of the decisions that turn out
to matter most.

## Context

Every part of this system that decides an amount has to agree with every other
part, on every machine, forever. A plan computed on a maintainer's laptop, a CI
runner and a contributor's clone must be byte-identical, because the plan's id
is a hash of its contents and a settled round is checked against it.

Floating point does not have that property. `0.1 + 0.2 != 0.3` is the famous
part; the part that matters here is that a percentage of a total, computed as a
float and rounded to base units, can produce a different last digit depending
on the order the additions happened in.

## Decision

**Money is a count of base units, held in a `u128`.** `money::Amount` wraps
one, and no `f64` ever touches a balance.

**Percentages are basis points, held in a `u16`.** `10_000` bps is 100%.
`FeeSchedule` carries `protocol_bps` and `treasury_bps`; `contributor_bps()` is
what is left, computed rather than stored, so the three cannot disagree.

**Attribution scores are integer milli-points, in a `u128`**, for the same
reason applied to weights rather than to amounts.

**Splits use the largest-remainder method.** `Amount::split_by_weights` gives
each weight its floor share, then hands out the leftover base units biggest
remainder first, ties broken by index. It **always sums back to exactly the
input**.

**Fees round down, and dust goes to contributors.** Never the other way round.

## What was rejected, and why

**Floats, or a decimal type.** A decimal library would remove the rounding
surprise but not the determinism question — it introduces a dependency into the
one place a dependency must never change its mind, and every serialised amount
would then depend on that library's formatting. Integers of base units are what
the chains themselves use, so this is not a representation choice so much as
declining to introduce a second one.

**Rounding to nearest on the fee split.** It is the fairer-looking rule and it
is the wrong one here. Rounding to nearest sometimes rounds *up*, which takes a
base unit from contributors and gives it to the protocol. A rule that
occasionally favours the party writing the rule is not a rule anybody should
accept. Rounding down is worse for the protocol every time, which is exactly
what makes it trustworthy.

**Dropping the dust.** Discarding the remainder makes the arithmetic simpler
and makes the plan not add up, which would destroy the one property every other
guarantee is checked against.

**Distributing the remainder randomly, or to the largest holder.** Both
conserve the total. Neither is reproducible from the plan alone — largest
remainder with index tie-breaking is, which means a contributor can recompute
their own share and get the same answer.

## Consequences

- **`u128` does not round-trip through JSON numbers**, which are doubles and
  lose precision above 2^53. Every amount in every serialised artifact — ledger
  entries, plans, receipts, `--json` output — is therefore a **decimal
  string**, via `money::u128_str`. A consumer that parses it as a number has a
  bug that will not show up until the amounts get large.
- **The conservation property is testable, and is tested exhaustively.**
  `money::proofs` and `payout::proofs` drive the split over its domain rather
  than over examples; `tests/adversarial.rs` asks whether it can be made to
  produce a *wrong* answer.
- **A change to `split_by_weights`, `compute_id` or the fee split is breaking
  even when it compiles**, because it changes what people receive. `RELEASING.md`
  says so, and it needs a record superseding this one.
- **Ties are broken by index, so item ordering is load-bearing.** The same
  ordering fixes Merkle indices under [0001](0001-pull-not-push.md).
- **A fee schedule that consumes all 10,000 bps is refused** rather than paying
  contributors nothing.

## Related

- [0006](0006-the-plan-id-excludes-presentation.md) — what the id hashes, and
  what it deliberately does not.
- `book/src/trust/invariants.md` — the same rules, stated for a reader who is
  deciding whether to trust them.
