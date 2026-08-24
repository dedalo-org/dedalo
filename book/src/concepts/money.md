# Money

This is the chapter to read if you are deciding whether to trust Dedalo with
funds. Everything in it is enforced by tests, and most of it is proved
exhaustively — see [What is proved](../trust/verification.md) for the
difference between those two words.

## Amounts are integers

```rust,ignore
pub struct Amount(u128);
```

An `Amount` is a count of **base units** of an [`Asset`][asset]: wei, satoshi,
USDC micro-units. The asset carries the `decimals` needed to render it for a
human, and that rendering happens at the edge, for display only.

Floating point never touches a balance. `0.1 + 0.2 != 0.3` in binary floating
point, and a payout system that cannot make three shares add up to the round is
a payout system that either creates money or loses it. Percentages are **basis
points** (`u16`, 10,000 = 100%), never floats, for the same reason.

> **Money** — `Amount::parse` converts a human decimal string like `"12.5"`
> into base units exactly once, at the boundary. There is no `f64` in the path
> from that call to the transaction.

## The fee schedule

A round is cut in a fixed order, off the top:

```text
gross
├── protocol_bps   → the network's Open Collective        (default 2.5%)
├── treasury_bps   → this project's own reserve           (default 15%)
└── the remainder  → contributors, by attribution weight  (default 82.5%)
```

`FeeSchedule::validate` refuses a schedule where the two fees reach 10,000 bps,
because contributors would receive nothing and that is never what somebody
meant to configure.

### Fees round down

Always, and in the contributors' direction. When `protocol_bps` of a gross
amount is not a whole number of base units, the fee is the floor and the
remainder stays in the pool that gets split across people.

This is the one place where an arbitrary choice had to be made and the
direction matters: the alternative rounds fractions of a base unit into the
protocol's pocket, on every round, forever. The choice is stated here, tested,
and proved over every fee schedule that validates — all 50,005,000 of them.

## Splitting

`Amount::split_by_weights` divides an amount across integer weights using the
**largest-remainder method**:

1. Give each recipient `floor(amount × weight / total_weight)`.
2. Whatever is left over — always fewer base units than there are recipients —
   goes one unit at a time to the recipients with the largest fractional
   remainders, ties broken deterministically.

The properties that follow, each with a test:

| Property | Meaning |
| --- | --- |
| **Conservation** | The shares sum to *exactly* the input. Not approximately. |
| **Zero weight, zero pay** | A weight of zero never receives a base unit. |
| **Monotonicity** | A larger weight never receives less than a smaller one. |
| **Determinism** | The same weights in the same order always split the same way. |

Conservation is the one that matters most, and it is why the method is
largest-remainder rather than "divide and round each". Rounding each share
independently loses or creates base units depending on which way the fractions
fell; the difference is small per round and unbounded over time.

### Proved, not sampled

- **Every basis-point value** — all 65,536 — rounds down and never exceeds its
  input.
- **Every weight vector** of length ≤ 4 with weights ≤ 6 — all 2,800 of them —
  conserves the total, never pays a zero weight, and never pays a larger weight
  less.
- **Every fee schedule that validates** — all 50,005,000 `(protocol_bps,
  treasury_bps)` pairs — cuts a round into three slices that sum to exactly the
  gross, with no fee rounded up.

Longer weight vectors and larger weights are *sampled* by property tests, not
proved. That distinction is recorded per module in `verification.toml` rather
than blurred, and the [verification chapter](../trust/verification.md) explains
why the line is drawn where it is.

## Nothing is created, nothing goes missing

A plan's transfers plus its `undistributed` field always equal exactly the
gross amount that funded it. There is no third possibility:

```text
gross == protocol fee + treasury + Σ contributor transfers + undistributed
```

`undistributed` is money that has no destination — the share of contributors
who have no wallet linked. It is **stated**, not absorbed into somebody else's
slice and not quietly dropped. Under the [pull model](../operating/multisig.md)
it is not even lost: it stays in the round until its owner claims it.

> **Careful** — a defect that made this false was real: a round in which nobody
> had a wallet silently dropped 82.5% of the funds. It is fixed, and
> `tests/adversarial.rs` now holds it down. Tests marked `FOUND:` in that file
> are regressions for defects that happened here, not hypotheticals.

## Overflow

`u128` is not infinite. Arithmetic in the money path uses checked or saturating
operations rather than wrapping ones, and a round large enough to overflow is
an error rather than a very small number. `verification.toml` counts the
arithmetic sites in each module and fails the build when the count changes, so
a new multiplication cannot be added to this path without somebody looking at
it.

[asset]: https://docs.rs/dedalo/latest/dedalo/money/struct.Asset.html
