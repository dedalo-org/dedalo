# Governance

Dedalo asks projects to route money using rules this repository defines, and
asks contributors to accept a payout computed by code it controls. Neither is a
reasonable ask without saying who decides things here.

This chapter is the short version. The authoritative documents are
[MAINTAINERS.md][maintainers] in the repository and [GOVERNANCE.md][gov] for
the organisation.

## Who decides

| Role | Who | What it means |
| --- | --- | --- |
| Maintainer | [@4137314](https://github.com/4137314) | Merges, releases, holds the crates.io ownership |

**One maintainer is the honest state today, and it is a weakness rather than a
structure.** It is written down so that it is visible, and so that the first
thing a second maintainer does is edit the table.

Concretely: no change here is reviewed by somebody other than its author. The
`main` ruleset requires an approving review and the maintainer bypasses it
*through a pull request*, never by pushing to the branch. A supply-chain
scorecard that flags this is making a true observation.

## How a change lands

Every change lands through a pull request, squash merged, with review threads
resolved. Pull request titles follow [Conventional Commits][cc] and become the
changelog entry — see [Releasing](releasing.md).

## Decisions that get written down

Some things are too consequential to live only in a merged diff. These get a
numbered record under [`docs/decisions/`][decisions] in the repository, and the
pull request links it:

- **anything that changes what people are paid** — the fee schedule, the
  attribution defaults, the split algorithm, the plan-id encoding;
- **anything that changes what a guarantee means** — the invariants, the
  verification methods, what a proof covers;
- **anything that gives the software a capability it deliberately lacks** —
  above all, holding a signing key.

A record says what was chosen, **what was rejected and why**, and what is now
load-bearing. The rejected alternatives are the part worth the most later,
because the obvious thing was usually considered and turned down for a reason
that is nowhere in the diff.

**They are binding.** If the code disagrees with a record, one of the two is
wrong, and the answer is not to quietly change the code.

Six exist today: the pull model, the absence of a signing key, Solana and the
address layer, integers and basis points, git as the history layer, and what
the plan id hashes.

## Decisions that need more than one person

Today none of these can happen, and that is the point of listing them:

- **Funding a round.** Money moves from a multisig whose signers are not one
  person; automation proposes and people sign. There is no key in CI and no
  configuration option that would put one there.
- **Deploying the claim program**, which requires a published independent audit
  first.
- **Changing where the protocol fee goes.**

If the project cannot find signers who are not one person, then it cannot
honestly run funded rounds — and the right answer is to say so rather than to
lower the threshold until it fits the people available.

## The protocol fee

Every round a project settles routes `fees.protocol_bps` — **2.5% by
default** — to the organisation's Open Collective at
[opencollective.com/dedalo](https://opencollective.com/dedalo). That is what
"the network funds itself" means.

Open Collective rather than a private account, because the ledger is public:
where the money went is a page anyone can read.

The default is a number in a config file that every adopting project can change
for itself. Changing the *shipped* default needs a decision record under the
rules above.

**Nothing flows there yet** — on-chain settlement is not live. See
[Settlement](../concepts/settlement.md).

[maintainers]: https://github.com/dedalo-org/dedalo/blob/main/MAINTAINERS.md
[gov]: https://github.com/dedalo-org/.github/blob/main/GOVERNANCE.md
[decisions]: https://github.com/dedalo-org/dedalo/tree/main/docs/decisions
[cc]: https://www.conventionalcommits.org
