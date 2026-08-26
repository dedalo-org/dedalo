# `dedalo.toml`

The funding policy. It lives at the repository root, it is **committed**, and
it is reviewed like any other change — because it decides what people are paid.

Every table is validated on load, and unknown keys are **rejected** rather than
ignored. A typo in a key name would otherwise silently fall back to a default,
and a default fee schedule is not a thing to arrive at by accident.

`dedalo init` writes a commented template. Below is every key it can contain.

---

## `[project]`

```toml
[project]
name = "my-project"
repository = "https://github.com/me/my-project"
open_collective = "my-project"
```

| Key | Required | Default | Meaning |
| --- | --- | --- | --- |
| `name` | **yes** | — | Project name, used in plans and reports. Part of the [plan id](../concepts/plans.md#the-id). |
| `repository` | no | — | Canonical repository URL. |
| `open_collective` | no | — | Open Collective slug this project self-funds through. |

## `[git]`

```toml
[git]
branch = "main"
lands_as = "merges"
ignore_subjects = ["chore(release)", "Merge branch"]
ignore_emails = ["noreply@github.com", "actions@github.com"]
```

| Key | Default | Meaning |
| --- | --- | --- |
| `branch` | `"main"` | Changes landing on this branch are what earn a payout. |
| `lands_as` | `"merges"` | What a landed change looks like here: `"merges"` (a merge commit) or `"commits"` (every commit on the branch's first-parent line). |
| `ignore_subjects` | `[]` | Changes whose subject **starts with** one of these are skipped entirely. |
| `ignore_emails` | `["noreply@github.com", "actions@github.com"]` | Emails that never receive a payout, however much they commit. |

### `lands_as`, and why it is not detected for you

GitHub offers three ways to land a pull request and **only one of them makes a
merge commit**:

| Merge method | Merge commit | `lands_as = "merges"` sees it |
| --- | --- | --- |
| Create a merge commit | yes | yes |
| Squash and merge | no — one ordinary commit | **nothing** |
| Rebase and merge | no — commits replayed | **nothing** |

Squash-and-merge is the default many projects pick, and on such a repository
`lands_as = "merges"` reports zero pending work on a history full of merged
pull requests. `dedalo scan` says so explicitly when it finds no merge commit
anywhere on the branch, rather than printing an empty table and letting you
conclude there is nothing to pay for.

Setting `lands_as = "commits"` pays for **every commit on the branch's
first-parent line**. A merge commit there still counts, and still brings in the
work on its second parent, so a history mixing both is not counted twice.

The trade is worth stating: `"commits"` pays for a direct push as readily as
for a pull request. On a branch that requires pull requests those are the same
thing. On one that does not, anybody with write access can write themselves a
payout — so a repository using `"commits"` should protect the branch it pays
for.

This is a setting rather than a detection because it decides what every
contributor receives. A tool that changed its mind about that between runs,
because a history happened to grow its first merge commit, would be worse than
one that asks.

`ignore_subjects` matches on prefix; `ignore_emails` matches exactly. No
globbing, no regular expressions — a pattern language here is a place for a
subtle mistake to hide, and what it decides is who gets paid.

> **Note** — the default `ignore_emails` covers GitHub's own noreply and Actions
> addresses. If your CI commits under a different address, add it. A bot with a
> wallet is a way for a round to leak.

## `[attribution]`

```toml
[attribution]
base_points = 100
points_per_insertion = 1.0
points_per_deletion = 0.5
max_points_per_merge = 5000
credit_merger = false
split_with_co_authors = true
```

| Key | Default | Meaning |
| --- | --- | --- |
| `base_points` | `100` | Flat score every merged pull request earns, regardless of size. |
| `points_per_insertion` | `1.0` | Score per added line. |
| `points_per_deletion` | `0.5` | Score per removed line. Deleting code is work too. |
| `max_points_per_merge` | `5000` | Ceiling per merge, so one vendored dependency cannot drain a round. |
| `credit_merger` | `false` | Also credit whoever pressed merge, on top of the commit authors. |
| `split_with_co_authors` | `true` | Share a commit's score with its `Co-authored-by:` trailers. |

The two per-line values are decimals in the file because that is how people
think about them, and they are converted to integer milli-points once, at load.
Nothing downstream sees a float. See [Attribution](../concepts/attribution.md).

> **Money** — changing anything in this table changes what people are paid, so
> it changes the [plan id](../concepts/plans.md#the-id) too. Under
> [the release policy](../contributing/releasing.md) a change to attribution
> defaults in Dedalo itself is a breaking change even when it compiles.

## `[asset]`

```toml
[asset]
symbol = "USDC"
decimals = 6
chain = "devnet"
contract = "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU"
```

| Key | Required | Meaning |
| --- | --- | --- |
| `symbol` | **yes** | Display symbol. |
| `decimals` | **yes** | Decimal places the token uses on chain. Every amount you type is converted to base units with this. |
| `chain` | **yes** | Chain identifier, e.g. `base`. Cross-checked against address formats. |
| `contract` | no | Token contract address. Omit for the chain's native coin. |

> **Careful** — `decimals` is not cosmetic. It converts `--amount 1000` into base
> units. Getting it wrong by one scales every round by ten, in whichever
> direction is worse.

## `[fees]`

```toml
[fees]
protocol_bps = 250     # 2.5% → the network's Open Collective
treasury_bps = 1500    # 15%  → this project's reserve
                       # 82.5% → contributors
```

| Key | Default | Meaning |
| --- | --- | --- |
| `protocol_bps` | `250` | Share routed to the Open Collective wallet that funds the network. |
| `treasury_bps` | `1500` | Share retained by the project for future rounds, audits, infrastructure. |

Basis points: 10,000 = 100%. A schedule where the two reach 10,000 is
**rejected**, because contributors would receive nothing and that is never what
somebody meant to configure.

Fees are taken off the top, protocol first, and they **round down** — the
remainder stays with contributors. Both properties are proved over every
schedule that validates. See [Money](../concepts/money.md#the-fee-schedule).

## `[wallets]`

```toml
[wallets]
source = "0x…"           # funds each round is paid out of
treasury = "0x…"         # this project's own reserve
open_collective = "0x…"  # the network's wallet, receiving protocol_bps
```

All three are **required** and all three are validated on load. `dedalo init`
writes the zero address as a placeholder, and settlement refuses to send to it.

> **Careful** — a placeholder is refused. A *wrong real address* is refused by
> nothing. Confirm each of these out of band before a round moves money, and
> read [how strong the checksum is](../concepts/identities.md#there-is-no-checksum)
> before deciding that a valid address is a correct one.

## `[settlement]`

```toml
[settlement]
backend = "dry-run"
# rpc_url = "https://…"
# cluster = "devnet"
# contract = "0x…"
```

| Key | Default | Meaning |
| --- | --- | --- |
| `backend` | `"dry-run"` | `dry-run` computes and verifies without spending. `evm` validates and builds the call, then refuses to sign. |
| `rpc_url` | — | JSON-RPC endpoint of the chain. |
| `cluster` | — | EIP-155 chain id, checked against the endpoint. |
| `contract` | — | Claim contract a round is deposited into. |

There is **no key here, and there must never be one.** `settlement.signer_env`,
which named an environment variable holding a signing key, was removed on
purpose. Dedalo does not sign; `dedalo propose` prints transactions for a
multisig. See [Funding from a multisig](../operating/multisig.md).

## `[[identities]]`

```toml
[[identities]]
handle = "ada"
wallet = "0xAdA0000000000000000000000000000000000000"
emails = ["ada@example.com", "ada@work.example"]
```

| Key | Meaning |
| --- | --- |
| `handle` | Label used in reports. Usually a GitHub username; nothing checks it. |
| `wallet` | Destination address, validated and checksummed on load. |
| `emails` | Every git author email this person commits under. |

Repeat the table for each contributor. Manage it with
[`dedalo identity`](cli.md#dedalo-identity) rather than by hand — the command
validates the address and reports how much that validation is worth.

One handle, one wallet, many emails: that is what makes
[one wallet, one transfer](../concepts/identities.md#one-wallet-one-transfer)
true.

---

## A complete example

```toml
[project]
name = "my-project"
repository = "https://github.com/me/my-project"
open_collective = "my-project"

[git]
branch = "main"
lands_as = "commits"
ignore_subjects = ["chore(release)"]
ignore_emails = ["noreply@github.com", "actions@github.com"]

[attribution]
base_points = 100
points_per_insertion = 1.0
points_per_deletion = 0.5
max_points_per_merge = 5000
credit_merger = false
split_with_co_authors = true

[asset]
symbol = "USDC"
decimals = 6
chain = "devnet"
contract = "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU"

[fees]
protocol_bps = 250
treasury_bps = 1500

[wallets]
source = "So11111111111111111111111111111111111111112"
treasury = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
open_collective = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"

[settlement]
backend = "dry-run"

[[identities]]
handle = "ada"
wallet = "0xAdA0000000000000000000000000000000000000"
emails = ["ada@example.com"]
```
