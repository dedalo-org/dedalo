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

## `dedalo completions <SHELL>`

Print a completion script for `bash`, `zsh`, `fish`, `powershell` or `elvish`
on stdout.

Hidden from the command list, because it is a setup step rather than something
anybody runs twice — `--help` names it at the bottom, which is where somebody
looking for it looks.

```console
$ mkdir -p ~/.local/share/bash-completion/completions
$ dedalo completions bash > ~/.local/share/bash-completion/completions/dedalo
```

| Shell | Where it usually goes |
| --- | --- |
| bash | `~/.local/share/bash-completion/completions/dedalo` |
| zsh | a directory on `$fpath`, as `_dedalo` |
| fish | `~/.config/fish/completions/dedalo.fish` |
| powershell | appended to `$PROFILE` |
| elvish | sourced from `~/.config/elvish/rc.elv` |

**Nothing installs these for you.** `cargo install` does not, and `install.sh`
prints the command rather than writing into your shell's configuration — a
script you piped to `sh` editing your dotfiles unasked is a bad habit for
everyone involved. The release archives ship a generated copy under
`completions/` for packagers.

The script is generated from the same argument definitions that parse the
command line, so it cannot list a flag that does not exist. That matters more
here than usual: several flags change what happens to money — `--execute`,
`--allow-undistributed`, `--since` — and getting one wrong from a typo is
exactly what completion prevents.

## `dedalo man`

Print the manual page, in roff, on stdout. Also hidden.

```console
$ dedalo man > /usr/local/share/man/man1/dedalo.1
$ dedalo man | man -l -          # read it without installing it
```

Release archives ship it at `man/man1/dedalo.1`.
