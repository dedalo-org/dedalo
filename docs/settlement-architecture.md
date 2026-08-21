# How funds move

The decisions on this page shape the whole settlement layer. They were taken
before any contract was written, because reversing them afterwards means
rewriting the part of the system that touches money.

**Status.** Decided, not built. Nothing here is implemented: the `evm` backend
validates a plan and stops before signing. See #8 and #9.

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

**Consequence.** `settlement.signer_env` describes a capability Dedalo should
not have. It stays for now because the `evm` backend is inert, and it should be
removed rather than implemented.

## Decision 3 — chain-agnostic, honestly

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

**Not decided:** which chain to launch on. The template currently names Base
and real mainnet USDC — a default that was never chosen deliberately and should
be, before anyone can broadcast. Testnet-first is the safer starting point.

## What has to exist before real funds move

1. A claim contract with the Merkle root, a per-round replay guard keyed on the
   plan id, and an expiry path for unclaimed funds.
2. An independent audit of it, published.
3. A Safe, with signers who are not one person.
4. A testnet round settled end to end, from `dedalo plan` to a claim.
5. Removal of `settlement.signer_env`, so the config cannot describe a key CI
   is meant to hold.

Until all five, the honest state of this project is what the code already says:
`Error::NotImplemented`.
