# Reporting a vulnerability

**Never in a public issue, and never in a discussion.** Not a suspected one,
not a "probably nothing".

**→ [Open a private advisory](https://github.com/dedalo-org/dedalo/security/advisories/new)**

## What counts as a security issue here

Dedalo computes and executes payments, so the list is wider than "remote code
execution". Treat any of the following as a security issue rather than an
ordinary bug:

- a payout plan that pays **the wrong amount**, **the wrong address**, or
  **twice**;
- a way to make a plan's id stay the same while its transfers change;
- a way to make attribution **credit someone who did not write the code**;
- anything that exposes, logs, or persists a signing key;
- a way to settle a plan the ledger should have refused.

Credit assigned to the wrong person is on that list deliberately. It is a
payment defect, and it is treated like one.

## What to include

- **What it can cause, concretely, in amounts.** "The protocol fee is
  overcharged by one base unit per round" is more useful than "rounding looks
  suspicious".
- **Steps to reproduce, ideally as a failing test.** The test suite has a place
  for it already: `tests/adversarial.rs` is where defects that were real become
  regressions.
- **The version or commit** you tested.

## What happens next

| | |
| --- | --- |
| Acknowledgement | within 72 hours |
| Assessment | within seven days |
| Credit | in the advisory, unless you prefer otherwise |

## Scope

**Supported:** the latest release and the `main` branch.

**Out of scope:** the security of chains, wallets, RPC providers or Open
Collective themselves; misconfigured `dedalo.toml` files in third-party
repositories; and key management on a user's own machine.

## Current status

On-chain broadcasting is **not live**. The `evm` backend validates and builds
the distributor call, then stops before signing. Until the distributor contract
is deployed and audited, no version of Dedalo can move funds on its own.

That narrows the practical attack surface considerably today — and it is
exactly why a finding in the arithmetic, the plan id, or the ledger is worth
reporting **now**, while it costs nothing to fix.

## Never post these anywhere

Private keys, seed phrases, or anything from a wallet. Nobody working on this
project will ever ask for one, and a revoked key is still a key someone can
learn from. Addresses are public and fine to paste; the thing that signs for
them is not.
