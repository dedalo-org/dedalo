# Exit codes and errors

## Exit codes

Two of them.

| Code | Meaning |
| --- | --- |
| `0` | Success. |
| non-zero | Anything else. The message on stderr says which. |

There is no numbered taxonomy of failures, and adding one would be a promise
this project cannot keep: an exit code is a public API, and the set of ways a
payout can fail is not stable enough to freeze into small integers. Scripts
should branch on the exit code and, when they need detail, read the `--json`
output of a **successful** run.

```bash
if out=$(dedalo plan --amount 1000 --json); then
  jq -r .id <<<"$out"
else
  echo "planning failed" >&2
  exit 1
fi
```

## The error type

The library returns `dedalo::Error`; the CLI wraps it with `anyhow` and adds
context aimed at somebody at a terminal. Every variant below is what a caller
can match on.

| Variant | Means | Usual fix |
| --- | --- | --- |
| `Io { path, source }` | A file could not be read or written. | Permissions, or a `.dedalo/` that was not committed. |
| `Git { args, stderr }` | The `git` binary ran and exited non-zero. | Run the printed command by hand; the arguments are included for exactly that. |
| `GitMissing` | No `git` executable in `PATH`. | Install git. Dedalo cannot work without one. |
| `GitParse { context, detail }` | Git succeeded but produced output this version cannot parse. | Report it — this is a bug, not a configuration problem. |
| `Config(msg)` | The config is valid TOML but semantically wrong. | Read the message; it names the key. |
| `ConfigParse { path, source }` | `dedalo.toml` is not valid TOML, or has the wrong shape. | The TOML error carries a span. Unknown keys are rejected, so check for a typo. |
| `ConfigNotFound(path)` | No `dedalo.toml` up the directory tree. | `dedalo init`, or `-C` at the right directory. |
| `Serde` | A ledger entry, plan or receipt could not be (de)serialised. | Usually a hand-edited object. See below. |
| `Amount { value, decimals }` | An amount is not a valid decimal at the asset's precision. | `--amount 1.5` on a 0-decimal asset, or a stray thousands separator. |
| `Address { value, reason }` | A payout destination is not a usable address. | Almost always a bad EIP-55 checksum — the reason says so. |
| `Overflow(what)` | Arithmetic on money or weights would have wrapped. | The round is too large for the asset's base units. This is a refusal, not a rounding. |
| `UnknownIdentity(email)` | A commit author has no wallet mapped. | `dedalo identity link`. |
| `Settlement { backend, reason }` | A backend refused to execute the plan. | The reason names exactly one rule. See below. |
| `LedgerCorrupt { id, reason }` | The chain does not hash to what it claims. | **Not** a parse failure. See below. |
| `NotImplemented { feature, hint }` | The capability exists in the API but is not live. | The hint says what to do instead. |

## `NotImplemented` is not a bug

```console
$ dedalo settle --plan ded106bd7281 --execute
error: evm broadcasting is not implemented yet in this release:
       use `dedalo propose` and execute the transactions from your multisig
```

The `evm` backend validates the configuration and builds the exact distributor
call the plan translates into, then stops. It does not return a fake receipt,
because a settlement path that lies is worse than one that is missing. See
[Settlement](../concepts/settlement.md).

## `LedgerCorrupt` means the record changed

```console
$ dedalo verify
error: ledger is corrupt at dedc41…: entry does not hash to its recorded id
```

This is the mechanism working, not failing. Each entry hashes over its parent,
so an entry edited after the fact breaks every id after it. Three causes, in
order of likelihood:

1. **`.dedalo/` was partly committed.** An object is missing from the clone.
   Check `git status` and whether anything under `.dedalo/` is ignored.
2. **An object was hand-edited.** Restore it from git history; do not "fix" the
   ids to match.
3. **Two branches recorded rounds independently and were merged.** The chain
   forked. Decide which history is real before settling anything else.

> **Careful** — the fix for a broken chain is never to edit ids until `verify`
> passes. That produces a ledger that verifies and is wrong, which is strictly
> worse than one that does not verify.

## Settlement refusals

Every one of these names exactly one rule, and a test asserts that no two
refusals share a sentence:

| Message | What it means |
| --- | --- |
| plan id does not match its contents | The plan was edited, or built by a different version. |
| this plan id was already settled | The ledger has it. A retry, working as intended. |
| a transfer would go to the zero address | A `[wallets]` placeholder was never filled in. |
| the round reaches nobody | The whole contributor pool is undistributed. `--allow-undistributed` if that is genuinely what you meant. |

The on-chain vault's refusals are listed under
[The refusals are the specification](../concepts/settlement.md#the-refusals-are-the-specification).

## Reporting one

A `GitParse` error, a panic, or an arithmetic result you believe is wrong is a
bug. Open an issue with the command, the output, and `dedalo --version`.

Anything where **the amount is or could be wrong** goes through
[SECURITY.md][sec] as a private advisory, never a public issue. That includes
credit assigned to the wrong person: it is a payment defect, and it is treated
like one.

[sec]: https://github.com/dedalo-org/dedalo/blob/main/SECURITY.md
