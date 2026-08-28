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

## The supply chain

Dedalo asks projects to run it in CI, where it reads their history and prints
transactions that move their money. "Why should I trust your supply chain" is
the right question to ask, and a list of good practices in a README is a claim
rather than evidence.

[OpenSSF Scorecard][sc] runs weekly and on every push to `main`, publishes to
the OpenSSF API, and uploads its findings to this repository's code-scanning
dashboard. The badge is in the README next to CI.

**A Scorecard number is about process, not about correctness.** It says nothing
about whether the arithmetic is right; the
[verification table](https://dedalo-org.github.io/dedalo/trust/verification.html)
is what speaks to that. The badge sits next to CI rather than next to the money
claims for that reason.

Three findings are expected, and two of them are true:

| Finding | Our answer |
| --- | --- |
| **Code-Review** — changes not reviewed by somebody other than the author | **True.** There is one maintainer, and `MAINTAINERS.md` says so out loud. Not a false positive; a weakness with a name. |
| **Branch-Protection** — scored from what the API can see | The `main` ruleset requires pull requests, squash merges, resolved threads and passing checks. Scorecard reads classic branch protection more completely than rulesets, so the score may understate it. `.github/rulesets/main.json` is the authority. |
| **Dangerous-Workflow** — `triage.yml` runs on `pull_request_target` | **Deliberate, and safe for the reason the check is worried about.** `pull_request_target` grants a write token; the hazard is checking out the pull request's code with that token in scope. `triage.yml` has **no checkout at all**, which is exactly why it is safe. Adding one would be the bug. |

A finding that is not on this list is a finding, and belongs in an issue.

[sc]: https://github.com/ossf/scorecard

## Current status

On-chain broadcasting is not live: the `evm` backend validates and builds the
distributor call, then stops before signing. Until the distributor contract is
deployed and audited, no version of Dedalo can move funds on its own.
