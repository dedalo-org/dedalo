# Your first round

The [quickstart](quickstart.md) produced numbers. This chapter produces numbers
you would be willing to defend, which is a different job: it is mostly about
identities, and about the two questions a plan makes you answer.

## Link the people

Attribution scores **git emails**, because that is what a commit carries. A
payout goes to a **wallet**. The mapping between them is the only part of
Dedalo that a human types in, and therefore the only part that can be wrong in
a way arithmetic cannot catch.

```bash
dedalo identity link ada 0xAdA0000000000000000000000000000000000000 \
  --email ada@example.com \
  --email ada@work.example
```

One handle, one wallet, as many emails as that person commits under. This is
what makes [one wallet, one transfer](../concepts/identities.md#one-wallet-one-transfer)
true: someone who commits from three machines is one payee, not three.

`identity link` validates the address before it writes it down, and tells you
how strong that check was:

```console
$ dedalo identity link ada 0xAdA0000000000000000000000000000000000000 --email ada@example.com
linked ada → 0xAdA0000000000000000000000000000000000000
warning: EIP-55 checksum carries 7 bits for this address
         a typo has roughly a 1-in-128 chance of surviving it
         confirm the address with ada through a second channel
```

That warning is not boilerplate. EIP-55 hides its checksum in the
capitalisation of the hex **letters**, so an address with few letters carries
few bits. See [Identities and wallets](../concepts/identities.md#how-strong-is-the-checksum).

## Find who is still missing

```console
$ dedalo identity missing
2 contributors have no wallet on file

  cy@example.com    1 merge    23.96% of the pending round
  dee@example.com   1 merge     4.10% of the pending round
```

Run this **before** you plan, every time. It is the difference between a round
you meant and a round you have to redo.

## The two questions a plan asks

### Is this the right split?

```console
$ dedalo plan --amount 1000
```

Read the `SHARE` column, not the `AMOUNT` column. The amounts follow from the
shares; the shares follow from `[attribution]`, and if a share looks wrong the
fix is in the config, not in the plan.

A merge that vendored a dependency and scored 5,000 points is the classic case.
That is what `max_points_per_merge` is for, and the moment to set it is now,
before the round rather than after it.

### Is anybody being dropped?

```console
$ dedalo plan --amount 1000 --json | jq '.unresolved'
[
  { "email": "cy@example.com",  "reason": "no identity links this email" },
  { "email": "dee@example.com", "reason": "no identity links this email" }
]
```

Nobody is ever silently dropped — but "reported" is not "paid". Two ways
forward, and they are a real choice:

| | What happens |
| --- | --- |
| Link them first, then plan | They are in the round. Requires reaching them. |
| Plan now | Their share stays in the round, unclaimed, until they link a wallet and claim it. |

The second is the point of the [pull model](../operating/multisig.md): a round
is deposited once against a Merkle root, and each contributor claims their own
share whenever they turn up. `undistributed` stops meaning "money with nowhere
to go" and starts meaning "not claimed yet".

> **Careful** — `dedalo settle --allow-undistributed` exists for the case where
> *nobody* in a round has a wallet and you meant to send the fees alone. If you
> find yourself reaching for it in a normal round, an `identity link` is
> missing and the flag is the wrong answer.

## Save the plan, then act on the saved one

```bash
dedalo plan --amount 1000 --save
```

`--save` writes the plan into `.dedalo/objects` and records it in the ledger.
From then on, refer to it by id:

```bash
dedalo propose --plan ded106bd7281
dedalo settle  --plan ded106bd7281
```

This matters more than it looks. Without `--save`, `settle --amount 1000`
recomputes the plan — and if a merge landed in the meantime, that is a
*different* plan from the one you reviewed. Saving first means the round people
approved is the round that executes.

## Commit the record

```bash
git add dedalo.toml .dedalo/
git commit -m "chore: fund round ded106bd7281"
```

`.dedalo/` belongs in git and must never go in `.gitignore`. A CI job clones
fresh; a runner that cannot see past rounds would pay them again.

## When it is real

Everything above ran against the `dry-run` backend and spent nothing. What has
to be true before a round moves actual funds is listed in
[Funding from a multisig](../operating/multisig.md) — the short version is a
deployed and audited claim contract, three real addresses, and signers who are
not one person.
