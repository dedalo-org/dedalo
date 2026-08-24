# Payout plans

A `PayoutPlan` is the artifact between git history and a transaction. It is
pure data: computed offline, reviewable in a pull request, and identified by a
hash of everything that determines it.

It exists so that the question "is this round correct?" can be answered by
reading a document, rather than by trusting the program that produced it.

## What is in one

```json
{
  "id": "ded106bd7281…",
  "project": "my-project",
  "created_at": 1766000000,
  "asset": { "symbol": "USDC", "decimals": 6, "chain": "base", "contract": "0x8335…" },
  "range": { "branch": "main", "from_commit": "af3141b5…", "to_commit": "7f10d55…", "merges": 4 },
  "split": {
    "gross": "1000000000",
    "protocol": "25000000",
    "treasury": "150000000",
    "contributors": "825000000"
  },
  "items": [
    { "kind": "contributor", "handle": "ada", "wallet": "0xAdA…",
      "amount": "514390000", "score": 1124000, "share_bps": 5144 },
    { "kind": "treasury",    "handle": "treasury", "wallet": "0x2222…",
      "amount": "150000000", "score": 0, "share_bps": 1500 },
    { "kind": "protocol",    "handle": "demo-collective", "wallet": "0x3333…",
      "amount": "25000000",  "score": 0, "share_bps": 250 }
  ],
  "undistributed": "197710000",
  "unresolved": [
    { "name": "Cy", "email": "cy@example.com", "score": 432000,
      "reason": "no-wallet" }
  ]
}
```

Amounts are strings because they are `u128` base units, and JSON numbers are
doubles. A number here would round silently above 2^53, which is well inside
the range of a token with 18 decimals.

## The id

```text
id = ded1 ‖ SHA-256(
       "dedalo.payout-plan.v1"
       ‖ project
       ‖ asset.symbol ‖ asset.chain ‖ asset.decimals ‖ asset.contract
       ‖ range.branch ‖ range.from_commit ‖ range.to_commit
       ‖ split.gross ‖ split.protocol ‖ split.treasury ‖ split.contributors
       ‖ undistributed
       ‖ for each item: kind ‖ wallet ‖ amount
     )[..16]
```

Every field is length-prefixed before it is hashed, so no two different plans
can serialise to the same byte string by moving a boundary — `("ab", "c")` and
`("a", "bc")` hash differently, which is exactly the kind of collision an
attacker would go looking for.

**What is deliberately outside the hash**, and why:

| Excluded | Why |
| --- | --- |
| `created_at` | The time you ran the command is not part of the answer. Including it would mean the same round computed twice never matched itself. |
| `handle` | A label for humans. Renaming `ada` to `ada-lovelace` does not change who is paid what — the wallet does. |
| `score` | Derived from the weights that produced the amounts; the amounts are what the plan commits to. |
| `range.merges` | A count of what the commit range already determines. |
| `unresolved` | Nobody in it is being paid. It is reported for review, and it is covered by `undistributed`, which *is* hashed. |

The encoding carries a version byte. Changing what goes into the hash changes
every plan id, so it is a breaking change under
[the release policy](../contributing/releasing.md) and the version byte is what
makes an old id and a new id distinguishable rather than merely different.

Three consequences, and they are the reason the id exists:

- **Reproducibility is checkable.** Anyone with the repository and the config
  recomputes the plan and compares one string. No diffing of tables.
- **Tampering is loud.** Change an amount, a handle, a wallet, or the fee
  split, and the id changes. Settlement re-derives the id from the contents
  before it does anything and refuses a plan whose id no longer matches.
- **Rounds are addressable.** `--plan ded106bd7281` refers to exactly one
  document, forever.

> **Note** — two different plans sharing an id would break all three. That is
> the first thing `tests/adversarial.rs` tries to construct.

## Saved plans

```bash
dedalo plan --amount 1000 --save
```

writes the plan into `.dedalo/objects` under its id and records a
`plan-created` entry in [the ledger](ledger.md). From then on:

```bash
dedalo propose --plan ded106bd7281
dedalo settle  --plan ded106bd7281
```

Do this for any round that will be reviewed by somebody other than the person
who ran it. `settle --amount 1000` recomputes a plan from current history; if a
merge landed between the review and the settlement, that is a **different
round** from the one that was approved. Referring to a saved id removes the
gap.

## Reviewing one

A plan is meant to be read. The order to read it in:

1. **`range`** — is this the range you meant? `from` should be the last settled
   commit, `to` the head you intend to pay for.
2. **`unresolved`** — is anybody in here who should have been linked?
3. **`share_bps`** — do the shares match what the project believes about who
   did what? Amounts follow from shares; a wrong share is a config problem.
4. **The sum.** Items plus `undistributed` must equal `gross`. The code
   guarantees it, and checking it once by hand is how you find out that you
   understand the document.
5. **`id`** — recompute it yourself and compare:

```bash
dedalo plan --amount 1000 --json | jq -r .id
```

## Why content addressing at all

The alternative is a sequential round number, and it is worse in a specific
way: a number says *when* a round happened, and says nothing about *what was
in it*. Two people holding "round 7" can hold different documents and not find
out. Two people holding `ded106bd7281` are holding the same bytes or they are
not holding it at all.

It also makes idempotence trivial to define. "Do not settle the same round
twice" becomes "refuse a plan id already in the ledger", which is an exact
check rather than a heuristic about dates and amounts. See
[The ledger](ledger.md#idempotence).
