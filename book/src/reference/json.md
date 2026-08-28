# JSON output

Every command takes `--json`. The shape is a **contract**, not incidental
output: `action.yml` parses it, and `tests/cli.rs` pins the fields it reads, so
renaming one fails the build instead of silently breaking the Action.

Treat it as a public API. Removing or renaming a field is a breaking change
under [the release policy](../contributing/releasing.md).

## Amounts are strings

```json
{ "gross": "1000000000", "undistributed": "197710000" }
```

Every amount is a `u128` count of base units, serialised as a **decimal
string**. JSON numbers are IEEE doubles: anything above 2^53 loses precision
silently, which is well inside the range of a token with 18 decimals.

Parse them as big integers, never as floats.

```bash
# right
jq -r '.split.gross' plan.json

# wrong — jq's numbers are doubles
jq '.split.gross | tonumber' plan.json
```

## `dedalo plan --json`

The serialised [`PayoutPlan`](../concepts/plans.md):

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
      "amount": "514390000", "score": 1124000, "share_bps": 5144 }
  ],
  "undistributed": "197710000",
  "unresolved": [
    { "name": "Cy", "email": "cy@example.com", "score": 432000, "reason": "no-wallet" }
  ]
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `id` | string | Content hash, prefixed `ded1`. |
| `project` | string | From `[project] name`. |
| `created_at` | number | Unix seconds. **Not** part of `id`. |
| `asset` | object | `symbol`, `decimals`, `chain`, optional `contract`. |
| `range.branch` | string | Branch the merges came from. |
| `range.from_commit` | string? | Exclusive lower bound. Absent on the first round. |
| `range.to_commit` | string | Newest merge in the round. |
| `range.merges` | number | How many merges the round covers. |
| `split` | object | `gross`, `protocol`, `treasury`, `contributors` — all base-unit strings. |
| `items[].kind` | string | `contributor`, `treasury` or `protocol`. |
| `items[].handle` | string | Label. |
| `items[].wallet` | string | Checksummed address. |
| `items[].amount` | string | Base units. |
| `items[].score` | number | Attribution weight in milli-points; `0` for fee recipients. |
| `items[].share_bps` | number | Share of the gross, for human review. |
| `undistributed` | string | Contributor pool that reached nobody. |
| `unresolved[].reason` | string | `no-wallet`, `excluded` or `ignored`. |

The invariant to assert in any script that consumes this:

```text
Σ items[].amount + undistributed == split.gross
```

## `dedalo contributors --json`

The serialised `Attribution`:

```json
{
  "contributions": [
    { "author": { "name": "Ada", "email": "ada@example.com" },
      "score": 1124000, "merges": 2, "commits": 5,
      "insertions": 592, "deletions": 50 }
  ],
  "merges_analysed": 4,
  "total_score": 1803000
}
```

`contributions` is ordered highest score first. `total_score` is the
denominator of the split — a contributor's share is `score / total_score`.

## `dedalo scan --json`

An array of `MergeEvent`, oldest first:

```json
[
  { "sha": "9c2f1ab…",
    "merged_by": { "name": "Ada", "email": "ada@example.com" },
    "merged_at": 1765900000,
    "subject": "feat(parser): streaming tokenizer",
    "commits": [
      { "sha": "3e1f…", "author": { "name": "Ada", "email": "ada@example.com" },
        "co_authors": [], "authored_at": 1765890000,
        "subject": "parse without buffering" }
    ],
    "diff": { "files_changed": 7, "insertions": 412, "deletions": 38 } }
]
```

## `dedalo status --json`

```json
{
  "project": "my-project",
  "branch": "main",
  "asset": { "symbol": "USDC", "decimals": 6, "chain": "base", "contract": "0x8335…" },
  "pending_merges": 4,
  "pending_contributors": 3,
  "fees": { "protocol_bps": 250, "treasury_bps": 1500, "contributor_bps": 8250 },
  "settlement_backend": "dry-run",
  "state": { }
}
```

## `dedalo verify --json`

```json
{
  "ok": true,
  "head": "dedc6ddbbe…",
  "entries": 4,
  "plans_checked": 2,
  "problems": []
}
```

`problems` carries `{ "plan": "…", "reason": "…" }` for anything that did not
check out. `ok` is also reflected in the exit code, so a script can branch on
either.

## `dedalo ledger --json`

An array of entries, plus `{ "migrated": N }` for `--migrate`.

## `dedalo propose --json`

The serialised `RoundProposal`:

```json
{
  "plan_id": "ded106bd7281…",
  "merkle_root": "0x…",
  "claim_contract": "0x…",
  "token": "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU",
  "total": "825000000",
  "claims": 3,
  "transactions": [
    { "step": 1, "description": "approve the claim contract to move 825 USDC",
      "chain_id": 8453, "to": "0x8335…", "value": "0", "data": "0x095ea7b3…" },
    { "step": 2, "description": "deposit round ded106bd7281 against its root",
      "chain_id": 8453, "to": "0x…", "value": "0", "data": "0x…" }
  ]
}
```

`total` is the sum of every claim, and it is what the deposit must cover
exactly. `transactions` is ordered: a deposit before its approval reverts.
`data` is what a signer compares against the plan — see
[what a signer should check](../operating/multisig.md#what-a-signer-should-check).

## `dedalo claim --json`

```json
{
  "plan_id": "ded1ebb700f156f6384dc99e6101b19c9271",
  "index": 2,
  "account": "MerkS3LaQBSvM5JZsvBaLZBBSMvMB5aTuLRHrvKAyDo",
  "handle": "bea",
  "amount": "46270331",
  "amount_display": "46.270331",
  "merkle_root": "0xba4aced424666c5c…",
  "proof": ["0xf04ae24d…", "0x63fdbd1f…", "0xe4ce5414…"],
  "claim_program": "MerkS3LaQBSvM5JZsvBaLZBBSMvMB5aTuLRHrvKAyDo"
}
```

`amount` is base units as a decimal string, as everywhere; `amount_display` is
the same number for a person, and a consumer building a transaction wants the
first. `proof` is ordered leaf upward and the order matters. `claim_program` is
absent when `dedalo.toml` names none, rather than being an empty string that
looks like an address.

`index` is a position among the plan's payable items — see
[the caveat in the command reference](cli.md#the-index-is-a-position).

## `dedalo identity link --json`

```json
{ "handle": "ada", "wallet": "0xAdA…",
  "emails": ["ada@example.com"], "checksum_bits": 15 }
```

`checksum_bits` is how much EIP-55 validation is worth for that address — see
[How strong is the checksum](../concepts/identities.md#there-is-no-checksum).

`identity remove` returns `{ "removed": "ada" }`.

## Errors

A command that fails writes a human message to stderr and exits non-zero. With
`--json`, stdout carries the machine-readable result of a **successful** run
only; do not parse stdout without checking the exit code first.

```bash
if out=$(dedalo plan --amount 1000 --json); then
  echo "$out" | jq -r .id
else
  echo "planning failed" >&2
  exit 1
fi
```

See [Exit codes and errors](errors.md).
