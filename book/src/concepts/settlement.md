# Settlement

The fourth stage, and the only one with side effects. It is mostly a list of
things it refuses to do.

## The backends

| Backend | What it does | Status |
| --- | --- | --- |
| `dry-run` | Re-verifies the plan and reports every transfer that would happen. Moves nothing. | **default** |
| `evm` | Validates the config, builds the exact distributor call the plan translates into, then stops before signing. | returns `NotImplemented` |

```toml
[settlement]
backend = "dry-run"
```

`dry-run` is the default because the safe thing should be the default. It
produces the same numbers a real settlement would; the difference is the
broadcast, not the arithmetic.

> **Careful** — the `solana` backend deliberately returns an error instead of
> pretending to broadcast. If you are reading the source and are tempted to
> "fix" that by returning a receipt: a settlement path that lies is worse than
> one that is missing. That refusal is the honest state of the project.

## What settlement refuses

Every one of these is a way money could otherwise be lost:

- **A plan whose id does not match its contents.** The id is re-derived from
  the plan before anything else happens. This catches an edited plan file, and
  it catches a plan built by a different version of the code.
- **A plan id already settled.** The ledger is consulted, and an exclusive lock
  is held for the duration, so a retry or a concurrent job cannot pay twice.
  See [Idempotence](ledger.md#idempotence).
- **A transfer to the zero address.** An unset placeholder in `[wallets]` is
  the common cause, and sending to it burns the money.
- **A round that reaches nobody.** If the whole contributor pool is
  undistributed, settlement stops. `--allow-undistributed` overrides it, and
  exists for the case where you genuinely meant to send only the fees.

## Dedalo holds no signing key

Not in CI, not in `dedalo.toml`, not on a maintainer's machine. There is no
flag that changes this, and there is no config key that names an environment
variable holding one — `settlement.signer_env` was removed on purpose and must
not come back.

What happens instead:

```bash
dedalo propose --plan ded106bd7281
```

prints the two transactions a round needs, with their calldata encoded, for
somebody to execute from a multisig:

```text
1. approve(claimContract, 1000000000)
   to     4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU
   data   0x095ea7b3…

2. deposit(planId, merkleRoot, token, 1000000000)
   to     11111111111111111111111111111111
   data   0xd0e30db0…
```

Nothing in that path opens a socket. A signer compares the printed calldata
against a plan they can read, rather than trusting a tool they cannot.

The reason is narrow and worth stating: **a key in CI can be reached by
everything that can write a workflow.** A pull request that edits a workflow
file, a compromised action, a dependency with a build script — each of those
becomes a path to the treasury the moment a key is in reach of a runner. There
is no key in reach of a runner.

## The pull model

A round is deposited **once**, against a Merkle root of its claims, and each
contributor claims their own share.

```text
dedalo plan     ─▶  a reviewed PayoutPlan, content-addressed
dedalo propose  ─▶  1. approve(claimContract, total)
                    2. deposit(planId, merkleRoot, token, total)
                        ↓
                    a multisig, signed by people who are not one person
                        ↓
                    contributors claim, each paying their own gas
```

Three holes in the obvious "loop over payees and send" design that this closes:

- **A contributor without a linked wallet is not a blocker.** Their share sits
  in the round until they claim it.
- **The project pays one transaction's gas**, not one per payee.
- **A key in CI cannot drain the treasury**, because there is no key in CI.

## The vault

The rules a deployed contract enforces live in
[`src/chain/vault`][vault] as ordinary Rust, and they are **pure**: no storage,
no clock, no caller. They take the state they need and return the state they
produce.

That is what makes them testable over their whole domain instead of by
deploying them somewhere and poking them. The deployable at
[`src/chain/contract`][contract] is an [Solana][stylus] crate that
compiles to WebAssembly and is deliberately thin — reading storage, moving a
token, knowing the time. A reader checking whether the rules are correct should
end up in `vault`, not in the binding.

### The refusals are the specification

`Refusal` has one variant per way the vault says no, each with a test:

| Refusal | Why it exists |
| --- | --- |
| `RoundExists` | Replay guard. A retried job proposing the same plan cannot fund it twice. |
| `RoundUnknown` | Nothing was deposited for this plan id. |
| `NothingToDeposit` | A round with no root or no total can never be claimed — money in, no way out. |
| `ShortDelivery` | The token delivered less than promised. A fee-on-transfer token does this, and the round would pay early claimants and strand the rest. |
| `AlreadyClaimed` | This index of this round is already paid. |
| `BadProof` | The proof does not put this claim in this round's tree. |
| `ExceedsRound` | The claim is larger than what the round still holds. |
| `NotExpired` | The claim window has not closed, so nothing may be swept. |
| `NotDepositor` | Only the account that funded a round may recover what is left. |
| `Inconsistent` | `claimed` exceeds `total` — unreachable through these functions, checked anyway, because it means something else wrote the state. |
| `Overflow` | Arithmetic would have wrapped. |

A test asserts that no two refusals share a sentence, so a revert reason
identifies exactly one rule.

The claim window is **180 days**, fixed rather than chosen by the depositor. A
depositor who could choose it could choose a window that closes before anybody
claims.

### The leaf encoding is pinned

`chain::merkle::the_leaf_encoding_has_not_moved` holds a root and a proof
against a fixed fixture. A deployed vault verifies proofs against that
encoding, so changing it silently would invalidate every round already
deposited. Changing it deliberately is fine — the commit has to say why.

## Status

**Unaudited and undeployed.** The vault's rules are tested; the deployable
compiles and fits the 24 KiB Solana limit with room to spare; nothing has been
deployed and no address in any shipped config is real.

[What has to exist before real funds move](../operating/multisig.md#before-real-funds-move)
is the list, and it is short enough to check.

[vault]: https://github.com/dedalo-org/dedalo/tree/main/src/chain/vault
[contract]: https://github.com/dedalo-org/dedalo/tree/main/src/chain/contract
[stylus]: https://arbitrum.io/stylus
