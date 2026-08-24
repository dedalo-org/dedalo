# Quickstart

Ten minutes, no money, no wallet, no chain. At the end you will have a payout
plan for a real repository and a ledger that proves it was not edited
afterwards.

## 1. Describe the project

```bash
cd my-project
dedalo init --open-collective my-project
```

That writes a commented `dedalo.toml` at the repository root. It is meant to be
**committed**: the file *is* the project's funding policy, and it should be
reviewed like any other change. See [`dedalo.toml`](../reference/configuration.md)
for every key.

The three addresses under `[wallets]` are zeroed placeholders. Leave them for
now — nothing in this chapter sends anything anywhere.

## 2. See what is unpaid

```console
$ dedalo scan
4 merges since af3141b5

  9c2f1ab  feat(parser): streaming tokenizer          ada     +412 -38
  1de77c0  fix(cli): honour NO_COLOR                  bea      +11  -4
  4a0b93e  docs: rewrite the configuration chapter    cy       +96 -21
  7f10d55  feat(money): largest-remainder splits      ada     +180 -12
```

`scan` reads merge commits on the branch named in `[git] branch`, starting
after the last settled commit. Nothing has been settled yet, so this is the
whole history.

> **Note** — nothing here reaches the network. Stages 1 to 3 of the pipeline
> read the repository and compute; only settlement has side effects.

## 3. Score them

```console
$ dedalo contributors
HANDLE                      MERGES   POINTS    SHARE
ada <ada@example.com>            2   1,124    62.35%
cy <cy@example.com>              1     432    23.96%
bea <bea@example.com>            1     247    13.69%
```

Scores are **milli-points**, integers, computed from the rules in
`[attribution]`: a flat score per merge, per-line scoring, a per-merge cap, and
`Co-authored-by:` splitting. Same history, same numbers, every machine. See
[Attribution](../concepts/attribution.md).

## 4. Price a round

```console
$ dedalo plan --amount 1000
Round ded106bd7281  4 merges on main → 7f10d55
Gross 1000 USDC

PAYEE            KIND         WALLET           SHARE  AMOUNT
ada              contributor  0xAdA00000000…  51.44%  514.39
cy               contributor  0xCy000000000…  19.77%  197.71
bea              contributor  0xBeA00000000…  11.29%  112.90
treasury         treasury     0x2222222222…   15.00%     150
demo-collective  protocol     0x3333333333…    2.50%      25
```

The fee schedule comes off the top first, then the rest is split by weight.
`ded106bd7281` is the plan's **id**: a hash over everything that determines the
outcome. It excludes the timestamp on purpose, so re-running this command gives
you the same id.

Try it. Then change one number in `[attribution]` and try again — the id moves,
because the answer moved.

## 5. Simulate the settlement

```console
$ dedalo settle --amount 1000
dry-run: 5 transfers, 1000 USDC, plan ded106bd7281
ok plan id matches its contents
ok transfers sum to the gross amount
ok no transfer to the zero address
nothing was broadcast
```

`settle` without `--execute` runs the whole settlement path against the
`dry-run` backend: it re-verifies the plan and reports exactly what would move.
The numbers are the ones a real settlement would use.

## 6. Check the record

```console
$ dedalo verify
head dedc6ddbbef5415e6dcbf805b60affd83c49
ok 2 entries hash to their recorded ids
ok 1 settled plan present and self-consistent
```

`.dedalo/` now holds a hash-chained ledger of what happened. Every entry names
its parent and hashes over it, so editing an old one breaks every id after it.
`verify` needs no network and no key — it is a check anyone with a clone can
run, which is the whole point. See [The ledger](../concepts/ledger.md).

## Every command takes `--json`

```console
$ dedalo plan --amount 1000 --json | jq '.items[] | {handle, amount}'
```

The JSON shape is a contract, not incidental output: `action.yml` parses it and
`tests/cli.rs` pins the fields it reads. See [JSON output](../reference/json.md).

## Next

- [Your first round](first-round.md) — the same thing with real identities and
  a real decision about who gets paid.
- [The pipeline](../concepts/pipeline.md) — what each stage is allowed to do.
- [In CI](../operating/ci.md) — where this belongs long term.
