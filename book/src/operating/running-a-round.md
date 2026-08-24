# Running a round

The operational checklist. It assumes the project is configured and identities
are linked; if not, start at [Your first round](../getting-started/first-round.md).

## Before

- [ ] `dedalo verify` passes on a **fresh clone**. If the ledger does not
      verify, stop — the range the next round covers is derived from it.
- [ ] `dedalo identity missing` is empty, or every name in it is a deliberate
      decision rather than an oversight.
- [ ] The three addresses in `[wallets]` are the ones you intend, confirmed out
      of band. Placeholders are all zeroes and settlement refuses them, but a
      *wrong real* address is refused by nothing.
- [ ] The funding source actually holds the amount, plus gas.

## Compute and save

```bash
dedalo plan --amount 1000 --save
```

`--save` writes the plan and records it in the ledger. Everything after this
refers to the plan **by id**, so the round that executes is the round that was
reviewed:

```console
Round ded106bd7281  4 merges on main → 7f10d55
```

## Review it in a pull request

```bash
git add .dedalo/
git commit -m "chore: propose round ded106bd7281"
git push -u origin round/ded106bd7281
```

The plan is JSON on purpose — it diffs. What reviewers should check is in
[Reviewing a plan](../concepts/plans.md#reviewing-one); the short version is
range, `unresolved`, shares, and that the numbers sum.

Reviewing a payout in the same place code is reviewed is most of the value.
The people who would notice that a share looks wrong are the people already
reading pull requests.

## Simulate

```console
$ dedalo settle --plan ded106bd7281
dry-run: 5 transfers, 1000 USDC, plan ded106bd7281
ok plan id matches its contents
ok transfers sum to the gross amount
ok no transfer to the zero address
nothing was broadcast
```

Run this against the saved plan, not against `--amount`. Recomputing here would
produce a different round if a merge landed since the review.

## Fund it

```bash
dedalo propose --plan ded106bd7281
```

Two transactions, printed with their calldata, for signers to execute from the
multisig. See [Funding from a multisig](multisig.md) for what each signer
should check before approving — this is the step where money actually moves,
and it is the step Dedalo cannot do for you by design.

## Record it

```bash
git add .dedalo/
git commit -m "chore: settle round ded106bd7281"
git push
```

The ledger entry is only useful once it is committed. A round recorded on one
laptop is a round the next CI job will compute again.

## After

- [ ] `dedalo verify` passes.
- [ ] `dedalo status` shows the round as settled and the pending range as empty.
- [ ] The ledger `HEAD` in your release note or README is updated, if you
      publish one.

## Cadence

Nothing in Dedalo has an opinion about how often you do this. Monthly is the
common shape and has two practical advantages: the range is small enough that a
reviewer can hold it in their head, and a mistake costs one month rather than
one year.

What does matter is that **a round covers a contiguous range with no gaps**.
The range is derived from the ledger, so this is automatic as long as the
ledger is committed and `--since` is not used to skip past commits.

> **Careful** — `--since` overrides the ledger's cursor. It is for recomputing
> a range you already understand, not for choosing which merges to pay for.
> Using it to skip a range means those merges are never paid, and nothing later
> notices.

## When something is wrong

| Symptom | Likely cause |
| --- | --- |
| `verify` fails | The ledger was edited, or an object is missing from the clone. |
| Plan id differs from the reviewed one | History moved, or the config changed. Diff `dedalo.toml`. |
| A contributor is missing | Their email is not linked — check `identity missing`. |
| Shares look wrong | `[attribution]`, usually `max_points_per_merge`. |
| `settle` refuses | Read the message; every refusal names exactly one rule. |

Nothing here is fixed by editing `.dedalo/` by hand. Editing it breaks the
chain, and the break is the mechanism working.
