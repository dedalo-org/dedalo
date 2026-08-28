# How funds move

This page used to hold four decisions and their reasoning. It was the right
shape when there were three; it stopped being one when "which decisions have
been taken" became a question you answered by reading prose.

**The decisions now live in [`docs/decisions/`](decisions/), one per file, and
they are still binding**: if the code disagrees with one, one of the two is
wrong, and the answer is not to quietly change the code. This page is the index
and the status board for the settlement layer.

## The decisions this layer rests on

| # | Decision | State in the code |
| --- | --- | --- |
| [0001](decisions/0001-pull-not-push.md) | A round is deposited once; contributors claim | Built, in Rust. `chain::merkle` produces the root and `chain::vault` holds the rules a deployed program enforces. Unaudited, undeployed. |
| [0002](decisions/0002-no-signing-key.md) | Dedalo holds no signing key | Built, by removal. `settlement.signer_env` is gone, the `evm` backend broadcasts nothing, and `dedalo propose` prints transactions for people to sign. |
| [0003](decisions/0003-solana-and-the-address-layer.md) | Solana, and an address layer that knows formats | Address layer built — `wallet::AddressKind`, one variant. The chain is **decided**; the leaf encoding and the deployable still speak EVM. |
| [0005](decisions/0005-git-is-the-history-layer.md) | The history layer is git, and says so | Built, and now deliberate rather than incidental. |

[0004](decisions/0004-integers-and-basis-points.md) and
[0006](decisions/0006-the-plan-id-excludes-presentation.md) are older than the
settlement layer and constrain it: amounts are integers, and the plan id is the
replay guard a deposit is keyed on.

**Nothing here has moved a coin.** Of the five prerequisites below, one is done
and the four that matter are not.

## What has to exist before real funds move

1. **A claim program** with the Merkle root, a per-round replay guard keyed on
   the plan id, and an expiry path for unclaimed funds — on Solana, holding SPL
   token accounts rather than an ERC-20 balance.
2. **An independent audit of it, published** — including the findings that were
   not fixed, and why.
3. **A multisig, with signers who are not one person.** Squads is the Solana
   equivalent of the Safe this list used to name; which one is chosen, and by
   whom, is still open.
4. **A devnet round settled end to end**, from `dedalo plan` to a claim.
5. ~~Removal of `settlement.signer_env`, so the config cannot describe a key CI
   is meant to hold.~~ **Done.**

Until the first four, the honest state of this project is what the code already
says: `Error::NotImplemented`.

## What exists now, and what it is worth

The vault is one implementation, in Rust, split so that the part which decides
anything is pure: `chain::vault` takes the state it needs and returns the state
it produces, reads no clock and no caller, and is therefore driven over its
whole domain by tests rather than by deploying it somewhere and poking it. The
deployable binds it to Arbitrum Stylus and does nothing else.

**Both are on their way out** under [0003](decisions/0003-solana-and-the-address-layer.md),
and it is worth being exact about what survives. The split does: a pure core
that decides, and a thin binding that reads storage and moves a token, is the
right shape on any chain, and the Solana program will have it. The refusals do:
they are statements about the pull model, not about the EVM. What does not
survive is everything underneath — `[u8; 20]` addresses, keccak leaves, ABI
encoding, and a 24 KiB compressed budget that was the reason the vault holds
raw address bytes in the first place.

That work was worth something even so. **It is not an audit.** Nobody outside
this repository has looked at it, it has never held a coin, and the reentrancy,
ERC-20 and expiry paths have been reasoned about by their author and tested by
their author — and the ones that matter will have to be reasoned about again,
against a different execution model, because reentrancy on Solana is not the
same hazard and account validation is a hazard the EVM does not have.

**What was given up** should be said plainly: the previous vault was Solidity,
and solc's model checker discharged all ten of its arithmetic conditions with a
solver — a stronger statement than any test. Rust has no equivalent that
terminates on this codebase. The rules are now in one language, tested with the
same machinery as the rest of the money path, and proved by nothing.

Treat it as a specification that happens to compile.
