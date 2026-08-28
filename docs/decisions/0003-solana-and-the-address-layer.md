# 0003 — Solana, and an address layer that knows formats

**Status.** Accepted. Extracted from `docs/settlement-architecture.md`. The
address-layer half was decided first; the chain was decided later and is
recorded here with it because the two constrain each other.

## Context

Two questions that look like one. *What shape does an address have in this
codebase?* and *which chain does this project settle on?*

The template shipped Base and mainnet USDC. Those were never a choice — they
were scaffolding that pointed at a real token so the pipeline had something to
name.

## Decision

**The address layer knows about address *formats*, not about one chain.**
`wallet::AddressKind` names a format; `Address` carries which one it is, and
comparison follows that format's rules. A config is cross-checked: an address
that is well-formed for the wrong chain is still one the funds cannot reach.

**The chain is Solana.**

## What was rejected, and why

**A trait with one implementation**, for the address layer. A trait here is
indirection nobody pays for. The enum says exactly as much as is true today,
and adding a chain is four mechanical edits the compiler points at — the module
docs list them.

**Staying on an EVM chain.** The deciding number is not the absolute fee but
its ratio to the amount moved. A round here is often a few dollars per
contributor — a merge is not a salary — and that ratio is what kills a
merge-to-earn tool on a chain where claiming costs cents. Solana puts a claim
in fractions of a cent, so a contributor owed two dollars receives
approximately two dollars.

Three things follow, each of which happened to be a problem:

- **Native USDC.** Circle issues USDC on Solana directly, so no bridge sits
  between a contributor and the asset a plan names.
- **One network, not a family.** "Which EVM chain" was not a question with an
  answer — Base, Arbitrum, Optimism and the rest differ in ways this project
  cannot rank, which is exactly why the default was never chosen.
- **Finality in seconds**, so a round can settle inside the pipeline run that
  produced it.

## Consequences, including the costs

**The Stylus vault is discarded, not ported.** `chain::vault` and its binding
are roughly six hundred lines written against a 24 KiB WebAssembly budget that
no longer applies. The rules they encode survive — the replay guard, the expiry
path, the refusals — because those were always about the pull model and not
about the EVM. What does not survive is `[u8; 20]` addresses, keccak leaves and
ABI encoding.

**One safety property is genuinely worse.** EIP-55 hides a checksum in the
capitalisation of an EVM address's hex letters, so a mistyped address is
usually rejected — around fifteen bits' worth, which is what
`Address::checksum_bits` reports. **A Solana address carries no checksum at
all.** Any thirty-two bytes are a valid public key, so base58 that decodes to
the right length is accepted, and `checksum_bits` honestly returns zero.

Two things blunt that, and neither is a fix:

- An address meant to hold tokens must be a real ed25519 point. Decompressing
  it rejects roughly half of random slips, and rejects every program-derived
  address someone pastes in by mistake. That is a **validity check, not a
  checksum**, and it must be described as one.
- [0001](0001-pull-not-push.md) already means a wrong address does not burn
  anything. The pull model was chosen for other reasons and pays for itself
  again here.

`dedalo identity link` says this out loud on Solana rather than printing a
smaller number.

**What counts as on-curve is pinned**, by `chain::wallet`'s fixture of byte
patterns and their expected verdict, because the boundary between "wallet" and
"program-derived address" is decided by a dependency and must not move
silently.

## Related

- [0001](0001-pull-not-push.md) — why a wrong address is recoverable.
- [0005](0005-git-is-the-history-layer.md) — the same "one implementation,
  named honestly" reasoning, applied to the other end of the pipeline.
