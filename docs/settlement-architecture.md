# How funds move

The decisions on this page shape the whole settlement layer. They were taken
before any contract was written, because reversing them afterwards means
rewriting the part of the system that touches money.

**Status.** Decided, and now partly built.

| Decision | State |
| --- | --- |
| Pull, not push | Built, in Rust. `chain::merkle` produces the root and `chain::vault` holds the rules a deployed contract enforces; `src/chain/contract` binds them to Arbitrum Stylus. Unaudited, undeployed. |
| The key is not in CI | Built, by removal. `settlement.signer_env` is gone, the `evm` backend broadcasts nothing, and `dedalo propose` prints transactions for people to sign. |
| Chain-agnostic addresses | Built. `wallet::AddressKind`, one variant. |
| The chain | **Decided: Solana.** Not built — the address layer, the leaf encoding and the deployable still speak EVM. |
| The history layer | **Decided: git, and named git.** Built, and now deliberate rather than incidental. |

Nothing here has moved a coin. Of the five prerequisites at the bottom of this
page, one is done and the four that matter — a deployed contract, an audit, a
multisig, a testnet round — are not.

## Where this started

The first sketch was a **push** model: the treasury sends one transfer per
contributor, batched into a single transaction by a distributor contract.

It is the obvious design, and it is wrong for this problem in three ways:

- **It needs a valid address for everyone, at the moment of payment.** A
  contributor who has not linked a wallet cannot be paid, and their share has
  nowhere to go. All of `plan.unresolved` and `plan.undistributed` exists to
  describe that hole rather than to fix it.
- **The project pays gas proportional to the number of payees.** A round with
  fifty contributors costs fifty transfers' worth of gas, whether or not those
  fifty people ever wanted the money on that chain.
- **A wrong address burns the funds.** Validation catches typos; it cannot
  catch an address that is well-formed and simply not yours.

## Decision 1 — pull, not push

**A round is deposited once, against a Merkle root of `(address, amount)`.
Contributors claim.**

| | Push | Pull |
| --- | --- | --- |
| Transactions per round | one batch, or N | one deposit |
| Gas payer | the project | the claimer |
| Unlinked contributor | blocks, or forfeits | claims whenever they link |
| Wrong address | funds destroyed | funds unclaimed, recoverable |
| Partial failure | possible | not expressible |

The shape fits what already exists. A `PayoutPlan` is content-addressed and
deterministic; a Merkle root over its items is the same idea in a form a
contract can verify. The plan id and the root travel together: the id says
*which round*, the root says *who is owed what*.

This dissolves #7 rather than patching it. "Undistributed" stops being money
with nowhere to go and becomes money not yet claimed — which is a state, not a
loss.

**Open questions.** How long a round stays claimable; what happens to an
unclaimed remainder when it expires; whether a contributor can assign their
claim to another address.

## Decision 2 — the key is not in CI

**The pipeline proposes; humans execute.** Funding a round is a transaction
from a Safe (or the equivalent on whatever chain), proposed by automation and
signed by people.

The earlier design put a signing key in an environment variable read by a CI
job. That key can drain the source wallet, and everything with write access to
a workflow can reach it — which on this repository includes Dependabot's pull
requests and anything that lands in `.github/`. A compromised workflow should
cost embarrassment, not the treasury.

This is why the pipeline hardening matters as much as the arithmetic: `zizmor`,
pinned action SHAs and the ban on interpolating expressions into `run:` blocks
exist because that boundary is what a key in CI would have crossed.

**Consequence.** `settlement.signer_env` described a capability Dedalo should
not have, and has been removed. The `evm` backend now validates chain settings
and refuses to broadcast, pointing at `dedalo propose`, which emits:

```text
1. approve(claimContract, total)                  → the token
2. deposit(planId, merkleRoot, token, total)      → the claim contract
```

with the calldata encoded, so a signer compares it against a plan they can
read instead of trusting a tool they cannot.

## Decision 3 — Solana, and an address layer that still knows formats

**The address layer knows about address *formats*, not about one chain.**

`wallet::AddressKind` names a format; `Address` carries which one it is, and
comparison follows that format's rules — EVM addresses compare
case-insensitively because EIP-55 puts a checksum in the capitalisation, and a
different chain will have different rules. A config is cross-checked: an
address that is well-formed for the wrong chain is still one the funds cannot
reach.

It is an enum with one variant, not a trait with one implementation. A trait
here would be indirection nobody pays for; the enum says exactly as much as is
true today, and adding a chain is four mechanical edits the compiler will
point at. The module docs list them.

**Decided: Solana.** The template's Base and mainnet USDC were never a
choice; they were scaffolding that pointed at a real token. The choice is now
made, and it is made on the one number this product lives or dies by.

**Fees, against the size of a payout.** A round here is often a few dollars per
contributor — a merge is not a salary. What matters is not the absolute fee but
its ratio to the amount moved, and that ratio is what kills a merge-to-earn tool
on a chain where claiming costs cents. Solana puts a claim in fractions of a
cent, so a contributor owed two dollars receives approximately two dollars. That
argument does not hold for every project on this chain; it holds unusually
strongly for this one.

Three things follow, each of which happened to be a problem:

- **Native USDC.** Circle issues USDC on Solana directly. No bridge sits between
  a contributor and the asset a plan names, and no bridge is a thing this
  project has to have an opinion about.
- **One network, not a family.** "Which EVM chain" was not a question with an
  answer — Base, Arbitrum, Optimism and the rest differ in ways this project
  cannot rank, which is exactly why the default was never chosen. Solana does
  not pose the question again next quarter.
- **Finality in seconds.** A round can settle inside the pipeline run that
  produced it, rather than becoming a thing somebody checks on later.

**What this costs, stated plainly.** The Stylus vault goes: `chain::vault` and
its binding are roughly six hundred lines written against a 24 KiB WebAssembly
budget that no longer applies, and they are discarded, not ported. The rules
they encode survive — the replay guard, the expiry path, the refusals — because
those were always about the pull model and not about the EVM.

**And one safety property is genuinely worse.** EIP-55 hides a checksum in the
capitalisation of an address's hex letters, so a mistyped EVM address is usually
rejected — around fifteen bits' worth, which is why `Address::checksum_bits`
exists to report it. **A Solana address carries no checksum at all.** Any
thirty-two bytes are a valid public key, so base58 that decodes to the right
length is accepted, and `checksum_bits` will honestly return zero.

Two things blunt that, and neither is a fix:

- An address meant to hold tokens must be a real ed25519 point. Decompressing
  it rejects roughly half of random slips, and rejects every program-derived
  address someone pastes in by mistake. That is a validity check, not a
  checksum, and it should be described as one.
- Decision 1 already means a wrong address does not burn anything. A claim
  nobody can make leaves the money in the round until it expires, rather than
  sending it somewhere unrecoverable. The pull model was chosen for other
  reasons and pays for itself again here.

`dedalo identity link` must say all of this out loud on Solana rather than
printing a smaller number.

## Decision 4 — git, and named git

**The history layer is git, the code says git, and that is a decision rather
than an omission.**

The tempting version of this is that a payout should derive from "a history
nobody can quietly rewrite" — with git as one instance — and that `git::`,
`GitBackend`, `[git]` and `Error::Git` are an implementation leaking into the
vocabulary.

The abstraction is real: everything downstream of `GitBackend` sees
`MergeEvent` values and never a git invocation, and the tests already substitute
another implementation. What is not real is the second implementation. Jujutsu,
Sapling, Pijul and Fossil each have something that means "this change landed",
and they do not agree that it is a commit with two parents — so `MergeEvent`,
first-parent diffing and a revision syntax would all have to become negotiable.
That is a redesign of the one part of the pipeline that decides who is owed
what, paid for now, on behalf of a user who does not exist.

The gap that does exist is inside git, not beyond it: attribution finds nothing
in a squash-merge repository, and this project's own `main` is squash-only. That
is a real defect affecting real users today, and the effort belongs there.

`GitBackend` stays a trait — substituting it in tests is worth the indirection
on its own. What it stops carrying is the implication that a second backend is
coming.

## What has to exist before real funds move

1. A claim program with the Merkle root, a per-round replay guard keyed on the
   plan id, and an expiry path for unclaimed funds — on Solana, holding SPL
   token accounts rather than an ERC-20 balance.
2. An independent audit of it, published.
3. A multisig, with signers who are not one person. Squads is the Solana
   equivalent of the Safe this list used to name; which one is chosen, and by
   whom, is still open.
4. A devnet round settled end to end, from `dedalo plan` to a claim.
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

**Both are now on their way out**, and it is worth being exact about what
survives. The split does: a pure core that decides, and a thin binding that
reads storage and moves a token, is the right shape on any chain, and the
Solana program will have it. The refusals do: they are statements about the
pull model, not about the EVM. What does not survive is everything underneath —
`[u8; 20]` addresses, keccak leaves, ABI encoding, and a 24 KiB compressed
budget that was the reason the vault holds raw address bytes in the first
place.

That work was worth something even so. It is not an audit. Nobody outside this
repository has looked at it, it has never held a coin, and the reentrancy,
ERC-20 and expiry paths have been reasoned about by their author and tested by
their author — and the ones that matter will have to be reasoned about again,
against a different execution model, because reentrancy on Solana is not the
same hazard and account validation is a hazard the EVM does not have.

**What was given up** should be said plainly: the previous vault was Solidity,
and solc's model checker discharged all ten of its arithmetic conditions with
a solver — a stronger statement than any test. Rust has no equivalent that
terminates on this codebase. The rules are now in one language, tested with the
same machinery as the rest of the money path, and proved by nothing.

Treat it as a specification that happens to compile.
