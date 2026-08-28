# Decision records

A decision record says why something is the way it is, at the moment it was
decided, by someone who still remembered the alternatives.

Code says what it does. A commit message says what changed. Neither survives
contact with the question a reviewer actually asks two years later, which is
almost always *"why not the obvious thing instead?"* — and the obvious thing
was usually considered and rejected for a reason that is nowhere in the diff.

These records exist because this project decides how much money people are
paid. A rule nobody can find the reason for is a rule the next maintainer will
change by accident.

## When one is required

[GOVERNANCE.md][gov] names three categories, and they are the rule here:

- **anything that changes what people are paid** — the fee schedule, the
  attribution defaults, the split algorithm, the plan-id encoding;
- **anything that changes what a guarantee means** — the invariants, the
  verification methods, what a proof covers;
- **anything that gives the software a capability it deliberately lacks** —
  above all, holding a signing key.

A pull request that does one of those links its record. A pull request that
does none of them does not need one, and writing one anyway is how the
directory becomes noise.

[gov]: https://github.com/dedalo-org/.github/blob/main/GOVERNANCE.md

## What a record contains

Four things, in a page:

1. **Context** — what was true when the question came up.
2. **Decision** — what was chosen, stated so it can be checked against code.
3. **What was rejected, and why** — the part that is worth the most later.
4. **Consequences** — what is now load-bearing, including the costs.

Short beats thorough. The value is that it is findable and dated, not that it
is exhaustive.

## Status

A record is **accepted** when merged. It is never edited to change its meaning:
if a decision is reversed, a *new* record supersedes it and both say so. A
record edited into agreeing with the code is a record that has stopped being
evidence.

## They are binding

If the code disagrees with a record, one of the two is wrong, and the answer is
not to quietly change the code. That is the same status
[`docs/settlement-architecture.md`](../settlement-architecture.md) has always
had, and this directory is that pattern made general.

## The records

| # | Decision | Status |
| --- | --- | --- |
| [0001](0001-pull-not-push.md) | A round is deposited once; contributors claim | Accepted |
| [0002](0002-no-signing-key.md) | Dedalo holds no signing key | Accepted |
| [0003](0003-solana-and-the-address-layer.md) | Solana, and an address layer that knows formats | Accepted |
| [0004](0004-integers-and-basis-points.md) | Money is integers; percentages are basis points | Accepted |
| [0005](0005-git-is-the-history-layer.md) | The history layer is git, and says so | Accepted |
| [0006](0006-the-plan-id-excludes-presentation.md) | The plan id hashes outcomes, not presentation | Accepted |

## Numbering

Next free number, four digits, never reused. A number that turns out to
describe nothing gets a record saying so rather than being deleted — a gap in
the sequence is a question, and this directory should not raise ones it does
not answer.
