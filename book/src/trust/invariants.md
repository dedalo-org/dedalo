# The invariants

Ten statements. Each one is a reason the project can be trusted with money,
each one has tests, and each one is a thing that would be a defect rather than
a preference if it stopped being true.

They are listed here in the order they matter to somebody deciding whether to
run a round.

---

### 1. Money is integers

`money::Amount` is a count of base units. **No `f64` ever touches a balance.**
Percentages are basis points (`u16`), never floats.

*Why:* `0.1 + 0.2 != 0.3` in binary floating point. A payout system whose
shares do not add up to the round either creates money or loses it, and neither
is recoverable by looking at the output.

→ [Money](../concepts/money.md#amounts-are-integers)

### 2. Splits conserve the total

`Amount::split_by_weights` uses the largest-remainder method and must sum back
to **exactly** the input. A plan's items always sum to its gross amount.

*Why:* rounding each share independently loses or creates base units depending
on which way the fractions fell. Small per round; unbounded over time.

*Proved* exhaustively for every weight vector of length ≤ 4 with weights ≤ 6.

### 3. Fees round down; dust goes to contributors

Never the other way round.

*Why:* the alternative rounds fractions of a base unit into the protocol's
pocket, on every round, forever. This is the one place an arbitrary choice had
to be made, and the direction is the whole of it.

*Proved* exhaustively over all 50,005,000 fee schedules that validate.

### 4. Plans are content-addressed

`PayoutPlan::id` hashes everything that determines the outcome, and
**deliberately excludes `created_at`**. Two runs over the same history and
config produce the same id.

*Why:* it is what makes a round checkable by somebody who does not trust the
person who published it. Recompute, compare one string.

→ [The id](../concepts/plans.md#the-id)

### 5. One wallet, one transfer

A contributor with several emails is merged into a single item before a plan is
finalised. Addresses compare **exactly**.

*Why:* base58 has one encoding per value, so an account has one written form and
two different strings are two different accounts. Folding case — which the
previous chain family required, because EIP-55 put a checksum in the
capitalisation — would merge two unrelated accounts into one payee here. Both
directions are the same invariant: the number of payees in a plan must be the
number of accounts, and a payout table that lies about how many people there
are is a payout table that pays the wrong ones.

→ [One wallet, one transfer](../concepts/identities.md#one-wallet-one-transfer)

### 6. Nobody is silently dropped

A contributor with no wallet appears in `plan.unresolved` with a reason, and
their share is accounted for in `undistributed`.

*Why:* the failure mode this replaces is a round that quietly pays out less
than it says, to fewer people than earned it, with nothing in the output
saying so. That defect was real here — see the `FOUND:` tests.

### 7. Rounds are idempotent

The ledger refuses to settle the same plan id twice and holds an exclusive lock
while settling. `DedaloClaim.deposit` refuses the same plan id on chain.

*Why:* a retried CI job must not pay twice. Two independent mechanisms, because
the failure mode is paying people twice out of a treasury.

→ [Idempotence](../concepts/ledger.md#idempotence)

### 8. Attribution is integer-scored

Scores are milli-points (`u128`), so the same history yields the same weights on
every machine.

*Why:* two contributors getting different shares from the same history on
different laptops, with neither able to prove the other wrong.

### 9. The ledger is a hash chain

Every entry in `.dedalo/objects` names its parent and hashes over it, so an
entry edited after the fact breaks every id since. `dedalo verify` checks it,
and needs **no network and no key**.

*Why:* an append-only file is append-only by convention. This is append-only by
arithmetic. Never write a path that appends without linking to the current
head.

→ [The ledger](../concepts/ledger.md)

### 10. Dedalo holds no signing key

`dedalo propose` prints transactions; people execute them from a multisig.
`settlement.signer_env` was removed on purpose — do not reintroduce a config
key that names one.

*Why:* a key in CI is reachable by everything that can write a workflow. A
compromised workflow should cost embarrassment, not the treasury.

→ [Why the key is not in CI](../operating/multisig.md#why-the-key-is-not-in-ci)

---

## If you are changing code near one of these

Add tests. That is not a formality:

- Anything touching `money`, `attribution`, `money::treasury` or `payout` needs
  a test proving the amounts still balance — including the awkward cases: zero
  weights, a single payee, amounts that do not divide.
- A new rule about what people are paid belongs in
  `src/money/proofs.rs`, `src/payout/proofs.rs` or `tests/adversarial.rs`, not
  only in a hand-picked example.
- A new module needs an entry in `verification.toml`, and the gate will not let
  it merge without one.

> **Careful** — do not weaken a test to make it pass. If an amount no longer
> balances, the arithmetic is wrong, not the assertion. This is the single most
> important line in the contributing guide.
