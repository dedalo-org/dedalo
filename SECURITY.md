# Security policy

Dedalo computes and executes payments. A bug here can cost real money, so
please treat anything in the following list as a security issue rather than an
ordinary bug:

- a payout plan that pays the wrong amount, the wrong address, or twice;
- a way to make a plan's id stay the same while its transfers change;
- a way to make attribution credit someone who did not write the code;
- anything that exposes, logs, or persists a signing key;
- a way to settle a plan that the ledger should have refused.

## Reporting

Report privately through
[GitHub Security Advisories](https://github.com/dedalo-org/dedalo/security/advisories/new).
Please include:

- what an attacker or a mistake can cause, concretely, in amounts;
- steps to reproduce, ideally as a failing test;
- the version or commit you tested.

You will get an acknowledgement within 72 hours and an assessment within seven
days. We will credit you in the advisory unless you prefer otherwise.

## Scope

Supported: the latest release and the `main` branch.

Out of scope: the security of chains, wallets, RPC providers or Open Collective
themselves; misconfigured `dedalo.toml` files in third-party repositories; and
key management on a user's own machine.

## Current status

On-chain broadcasting is not live: the `evm` backend validates and builds the
distributor call, then stops before signing. Until the distributor contract is
deployed and audited, no version of Dedalo can move funds on its own.
