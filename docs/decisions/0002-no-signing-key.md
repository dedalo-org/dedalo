# 0002 — Dedalo holds no signing key

**Status.** Accepted, and enforced by removal. Extracted from
`docs/settlement-architecture.md`.

## Context

The first settlement design had `settlement.signer_env`: a config key naming an
environment variable that held a private key, which a CI job read and signed
with. That is how most automation moves money, and it is why most automation
that moves money is a liability.

## Decision

**The pipeline proposes; humans execute.** Funding a round is a transaction
from a multisig, proposed by automation and signed by people. Dedalo holds no
key, and the configuration has no way to describe one.

`dedalo settle` publishes the round and opens the proposal. It never moves the
money. `dedalo propose` prints the transactions with their calldata:

```text
1. approve(claimContract, total)                  → the token
2. deposit(planId, merkleRoot, token, total)      → the claim contract
```

so a signer compares calldata against a plan they can read, rather than
trusting a tool they cannot.

## What was rejected, and why

A key in a CI secret. The argument against it is not that it might leak — it is
what the blast radius is when it does.

**A key in CI can drain the source wallet, and everything with write access to
a workflow can reach it.** On this repository that surface includes Dependabot
pull requests and anything that lands in `.github/`. A compromised workflow
should cost embarrassment, not the treasury.

A narrower variant — a key with a spending cap, or one that can only call
`deposit` — was not taken either. It moves the question from "can this key
steal everything" to "is the cap configured correctly on a chain nobody has
deployed to yet", and the answer to the second is not checkable from here.

## Consequences

- **`settlement.signer_env` was removed from the config**, so it cannot
  describe a capability Dedalo should not have. Reintroducing a config key that
  names a signing key is a change to this record, not a feature.
- **The `evm` backend validates chain settings and refuses to broadcast**,
  returning `Error::NotImplemented` and pointing at `dedalo propose`. It does
  not return a fake receipt. A settlement path that lies is worse than one that
  is missing.
- **Pipeline hardening became load-bearing rather than hygiene.** Pinned action
  SHAs, `zizmor`, and the ban on interpolating `${{ }}` into `run:` blocks all
  exist because that is the boundary a CI-held key would have crossed. They stay
  even though the key is gone, because the reasoning survives the key.
- **Rounds now depend on people being available to sign.** A round nobody signs
  is a round that silently does not happen, and that is a real operational cost
  accepted deliberately.
- **The multisig becomes the trust anchor.** "No key in CI" is worth nothing if
  the multisig is one person holding every key — which is why the signer set is
  its own open question rather than an implementation detail.

## Related

- [0001](0001-pull-not-push.md) — the deposit this proposes is a single
  transaction because the model is pull.
- `book/src/operating/multisig.md` — what a signer should check before
  approving.
