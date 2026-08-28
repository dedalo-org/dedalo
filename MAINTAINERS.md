# Maintainers

Dedalo asks projects to route money using rules this repository defines, and
asks contributors to accept a payout computed by code it controls. Neither is a
reasonable ask without saying who holds what.

## Who

| Person | Merges | Releases | crates.io owner | GitHub org owner |
| --- | --- | --- | --- | --- |
| [@4137314](https://github.com/4137314) | yes | yes | yes | yes |

**One maintainer is the honest state today, and it is a weakness rather than a
structure.** It is written here so that it is visible, and so that the first
thing a second maintainer does is edit this table.

The consequences are concrete rather than theoretical:

- **No change is reviewed by somebody other than its author.** The `main`
  ruleset requires an approving review, and the maintainer bypasses it *through
  a pull request* — never by pushing. That bypass is documented in
  [GOVERNANCE.md][gov] and is the reason a Scorecard "Code-Review" finding here
  is a true observation, not a false positive.
- **Bus factor is one.** If this account goes away, the crate, the tag and the
  organisation go with it.
- **Nothing that needs more than one person can happen yet** — see below.

## What each capability means

**Merge.** Anyone with write access to the repository can merge a green pull
request. Today that is the maintainer.

**Release.** Dispatching the **Version** workflow and reviewing the release
pull request it opens. Merging that pull request tags `main` and publishes.
There is no manual publish step and no registry token — crates.io Trusted
Publishing mints a thirty-minute token against a signed OIDC claim, scoped to
this repository and the `crates-io` environment. See [RELEASING.md](RELEASING.md).

**crates.io ownership.** Who can yank a version, add an owner, or configure
Trusted Publishing. A published version can be yanked but never withdrawn,
which is why this is listed separately from "can release".

## What needs more than one person

Today none of these can happen, and that is the point of listing them:

- **Funding a round.** Money moves from a multisig whose signers are not one
  person; automation proposes and people sign. Dedalo holds no key and the
  configuration has no way to describe one — see
  [0002](docs/decisions/0002-no-signing-key.md).
- **Deploying the claim program**, which requires a published independent audit
  first.
- **Changing where the protocol fee goes.**

If the project cannot find signers who are not one person, then it cannot
honestly run funded rounds, and the right answer is to say so rather than to
lower the threshold until it fits the people available.

## The protocol fee

`fees.protocol_bps` defaults to **250** — 2.5% — and routes to the
organisation's Open Collective at
[opencollective.com/dedalo](https://opencollective.com/dedalo). That is what
"the network funds itself" means.

- **Open Collective rather than a private account**, because its ledger is
  public: where the money went is a page anyone can read.
- **The default is a number in a config file**, and every adopting project can
  change it for itself.
- **Changing the shipped default needs a decision record**, because it changes
  what people are paid — see [docs/decisions/](docs/decisions/).
- **Nothing flows there yet.** On-chain settlement is not live.

## Becoming a maintainer

There is no ladder to climb and no probation period, because there is nobody to
administer one. The practical route is a few substantive pull requests, and
then asking. What the role costs is honest review of changes that decide how
much money moves, which is the scarce thing here.

## Escalation

Security reports go to the process in [SECURITY.md](SECURITY.md), not to a pull
request. Everything else is an issue or a discussion.

[gov]: https://github.com/dedalo-org/.github/blob/main/GOVERNANCE.md
