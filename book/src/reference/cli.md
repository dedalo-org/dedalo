# Command line

```console
$ dedalo --help
Turn code merges into sustainable open-source funding
```

Every command accepts the global options below, and every command that produces
data accepts `--json`.

## Global options

| Option | Meaning |
| --- | --- |
| `-C, --repo <PATH>` | Repository to operate on. Defaults to the current directory. |
| `--json` | Emit machine-readable JSON instead of tables. See [JSON output](json.md). |
| `-v, --verbose` | Increase log verbosity. Repeatable. |
| `-h, --help` | Help for the binary or a subcommand. |
| `-V, --version` | Version. |

Dedalo finds `dedalo.toml` by walking up from `--repo` (or the current
directory), the way git finds `.git`. The directory holding it is the
repository root for everything that follows.

## Range options

`scan`, `contributors`, `plan`, `settle`, `propose` and `identity missing` all
take the same pair:

| Option | Meaning |
| --- | --- |
| `--since <REV>` | Start after this revision instead of the last settled commit. |
| `--limit <N>` | Show at most this many entries. |

> **Careful** — `--since` overrides the ledger's cursor. It is for recomputing a
> range you already understand. Using it to skip merges means those merges are
> never paid for, and nothing later notices.

---

## `dedalo init`

Create a `dedalo.toml` in this repository.

| Option | Meaning |
| --- | --- |
| `--name <NAME>` | Project name. Defaults to the repository directory name. |
| `--open-collective <SLUG>` | Open Collective slug that receives the protocol fee. |
| `--force` | Overwrite an existing `dedalo.toml`. |

The file it writes is commented, and it is meant to be committed and reviewed.

## `dedalo scan`

List merges that have not been paid out yet.

Reads merge commits on the branch named in `[git] branch`, starting after the
last settled commit in the ledger. Takes the [range options](#range-options).

## `dedalo contributors`

Show contribution scores for the pending range, in milli-points and as a share.

Takes the [range options](#range-options). This is `plan` without the money:
useful for showing people where they stand before a round is funded.

## `dedalo plan`

Compute a payout plan for a funding round.

| Option | Meaning |
| --- | --- |
| `--amount <AMOUNT>` | **Required.** Size of the round, as a decimal amount of the configured asset. |
| `--save` | Store the plan in `.dedalo` and record it in the ledger. |

Plus the [range options](#range-options).

`--save` is what makes a round referable by id afterwards. Use it for any round
somebody other than you will review — see
[Running a round](../operating/running-a-round.md).

## `dedalo settle`

Execute a payout plan. **Simulates unless `--execute` is given.**

| Option | Meaning |
| --- | --- |
| `--plan <PLAN_ID>` | Settle a plan that was already saved, by id. |
| `--amount <AMOUNT>` | Compute a fresh plan of this size and settle it. Required unless `--plan`. |
| `--execute` | Actually broadcast, using the backend from `dedalo.toml`. |
| `--allow-undistributed` | Settle even though the contributor pool reached nobody. |

Plus the [range options](#range-options). `--plan` and `--amount` conflict.

`--execute` does not broadcast today: no backend can sign, because
[Dedalo holds no signing key](../concepts/settlement.md#dedalo-holds-no-signing-key).
The `solana` backend builds the exact call and returns `NotImplemented`.

`--allow-undistributed` is only meaningful when *nobody* in the round has a
wallet on file, which normally means an `identity link` is missing rather than
that you meant to send the fees alone.

## `dedalo propose`

Emit the transactions a multisig must run to fund a round. Dedalo signs nothing
and holds no key.

| Option | Meaning |
| --- | --- |
| `--plan <PLAN_ID>` | Propose a plan that was already saved, by id. |
| `--amount <AMOUNT>` | Compute a fresh plan of this size and propose it. Required unless `--plan`. |
| `--save` | Store the plan before proposing it, so the round the signers execute is the one on disk. |

Plus the [range options](#range-options). `--plan` and `--amount` conflict.

Prints `approve` and `deposit` with their calldata encoded. See
[what a signer should check](../operating/multisig.md#what-a-signer-should-check).

## `dedalo claim --plan <ID> <WALLET_OR_HANDLE>`

What a contributor needs in order to be paid: their **index**, their **amount**
and their **proof path**.

```console
$ dedalo claim --plan ded1ebb700f1 ada
round      ded1ebb700f1
index      0
amount     721.156157 USDC
account    4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU
root       0xba4aced424666c5c01a48872676cb7ba824c4b449ff4bd2c733a89d29fae0739
proof      0x60b145dcce113f3d6acba49476ee4a607b31a816d97a8bc1e2af061837376f8c
           0xe819397c96a504c4718e3c5b74bd4f23702d3cf563f458ffeba4d328f3e360ee
           0xe4ce5414a9b8593225ee813208f464e830bc0a31f857655eaffdedcd44391a9e
```

Accepts a **wallet address or a handle**. The address is what a claimer
certainly knows; the handle is what they see in a plan. Both resolve to the
same item, and `--json` emits the whole thing for a front end to build a
transaction from.

### It needs no network, and no maintainer

Everything comes from a plan in `.dedalo/objects` and the config beside it. A
contributor clones the repository and derives their own proof — they do not ask
the maintainer to send them a blob. The threat model claims a contributor can
audit a project without asking permission; claiming should be no different.

### The proof is verified before it is printed

`chain::merkle` can check a proof against the root it derived, so `claim` does,
and refuses rather than printing one that does not verify. An unverified proof
sends somebody to a chain to find out, and gas is not refunded for that.

The plan is re-verified too: a plan that does not hash to its own id is one
somebody edited, and a proof derived from it would verify against nothing.

### The index is a position

A claim's index is **where it sits among the plan's payable items**, and it is
hashed into the leaf — so a proof is only valid while that ordering is fixed.
An index that moved would not merely fail to verify; it would address somebody
else's amount.

`chain::merkle::index_stability` pins it, with three tests: a plan read twice
yields the same indices and the same root, a proof verifies for its own index
and no other, and swapping two payees changes the root.

Sorting `items` for display, in place, is the change that would break this and
look harmless in review.

## `dedalo status`

Show the current funding state of the project: the configured asset and fee
split, the pending range, the last settled round, and the lifetime total paid.

## `dedalo verify`

Recompute the ledger chain and confirm nothing was changed after the fact.

Needs no network, no key and no credentials — anyone with a clone can run it.
Exits non-zero if the chain does not verify, so it belongs in CI. See
[The ledger](../concepts/ledger.md#verifying-it).

## `dedalo ledger`

Print the event ledger.

| Option | Default | Meaning |
| --- | --- | --- |
| `--limit <N>` | `20` | Show only the last N entries. |
| `--migrate` | | Convert a pre-chain `ledger.jsonl` into chain entries, then stop. |

## `dedalo identity`

Manage the git-identity to wallet mapping.

### `identity list`

List known identities.

### `identity link <HANDLE> <WALLET> --email <EMAIL>...`

Map one or more git emails to a wallet.

| Argument | Meaning |
| --- | --- |
| `<HANDLE>` | Handle used in reports, e.g. a GitHub username. |
| `<WALLET>` | Destination wallet address. |
| `--email <EMAIL>` | Git author email to attach. **Repeatable, and required.** |

Validates the address and reports how many bits of EIP-55 checksum protect it —
see [How strong is the checksum](../concepts/identities.md#there-is-no-checksum).

### `identity remove <HANDLE>`

Remove an identity by handle. Does not touch history: the person's past rounds
stay in the ledger, and future rounds list them as unresolved.

### `identity missing`

Show contributors in history that have no wallet yet. Takes the
[range options](#range-options).

Run this before every round.
