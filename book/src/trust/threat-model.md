# Threat model

Who could make Dedalo pay the wrong person, or the wrong amount, and what stops
them.

This is written from the attacker's side on purpose. A list of features is easy
to feel good about; a list of attacks is what tells you whether the features are
the right ones.

## What is being protected

1. **The amounts.** That the round pays what the history and the config say.
2. **The destinations.** That money reaches the people who earned it.
3. **The record.** That what was paid can be checked afterwards by somebody who
   was not there.

Notably **not** protected, because it cannot be: that the funding source has
money in it, that the multisig signers are honest, or that the project's
maintainers score contributions fairly. Those are governance, and Dedalo makes
them visible rather than solving them.

---

## A contributor games attribution

**Attack.** Inflate your own score: vendor a dependency, reformat the tree, add
generated files, split one change across twenty merges.

**What stops it, partly.** `max_points_per_merge` caps any single merge.
`base_points` means twenty small merges are worth more than one large one — so
splitting is *rewarded*, and that is a real limitation rather than a defence.

**What does not stop it.** Nothing prevents a contributor from writing verbose
code, and no line-counting formula can. The mitigation is social and it is the
same one the project already has: **the merge had to be reviewed**. Dedalo pays
for merged code, and a project that merges padding has a review problem, not an
attribution problem.

Set `max_points_per_merge` before the round rather than after it.

## A maintainer edits the record

**Attack.** Change an old ledger entry to say a round paid somebody it did not,
or to hide one that happened.

**What stops it.** The ledger is a hash chain. Editing an entry changes its id;
every later entry named the old id, so their ids change too, and `HEAD` stops
matching. `dedalo verify` catches it on any clone, with no network and no key.

**Residual.** A maintainer can rewrite the whole chain *and* force-push, which
is visible as a force-push and as a `HEAD` that does not match anything anybody
previously saw. Publishing the ledger `HEAD` in release notes makes that
detectable rather than merely possible to detect.

## Someone substitutes a wallet address

**Attack.** A contributor's address is changed — in a pull request to
`dedalo.toml`, or in transit before the maintainer pastes it.

**What stops it, barely.** Less than it used to. A Solana address carries **no
checksum**, so a typo is caught only when it changes the decoded length;
`identity link` reports [that the check is worth nothing][bits] rather than
implying otherwise. What it does refuse is an address nobody can sign for —
off-curve, and therefore not a wallet.

**What does not stop it.** A well-formed address is a *valid* address, not a
*correct* one. An attacker substituting a valid address of their own passes
every check Dedalo makes, and on this chain a careless typo does too.

**What limits the damage.** The pull model: a round is claimed, not sent, so a
share nobody can claim stays in the round until it expires rather than being
transferred somewhere unrecoverable. That is recovery, not prevention.

**Mitigation, which is procedural:** the change to `dedalo.toml` is a reviewed
commit with an author, and the address should be confirmed with the contributor
through a second channel. This is the largest residual risk in normal operation
and it is not a cryptographic problem.

## A CI job is compromised

**Attack.** A malicious pull request, a compromised action, or a dependency's
build script gets code execution in the workflow that funds rounds.

**What stops it.** There is nothing to steal. **Dedalo holds no signing key** —
not in CI, not in config, not in an environment variable. The config key that
named one was removed and must not come back. The worst outcome is a wrong plan
being *proposed*, and a proposal has to be read and signed by people.

This is the single largest design decision in the project, and it is why the
workflow hardening (pinned action SHAs, `zizmor`, no `${{ }}` in `run:`) matters
as much as the arithmetic.

## Someone tampers with a plan between review and settlement

**Attack.** The plan is approved in a pull request; a different plan is
settled.

**What stops it.** Plans are content-addressed. Settlement re-derives the id
from the plan's contents and refuses one that does not match, and the operator
refers to the round by id (`--plan ded1…`) rather than recomputing it.

**Residual, and it is an operator error rather than an attack:** running
`settle --amount 1000` instead of `settle --plan ded1…` recomputes from current
history. If a merge landed since the review, that is a different round and
nothing objects. [Save the plan and settle by id](../operating/running-a-round.md#compute-and-save).

## A round is paid twice

**Attack.** Re-run the settlement job, or run two concurrently.

**What stops it.** Three mechanisms:

1. The ledger refuses a plan id already recorded as settled.
2. An exclusive lock is held for the duration, so two jobs cannot both pass the
   check.
3. `DedaloClaim.deposit` refuses a plan id it has already seen, on chain.

Two of those are independent of each other, which is the point.

## A malicious token

**Attack.** The configured asset is a token that takes a fee on transfer, or
reverts selectively, or re-enters on transfer.

**What stops it.** The vault refuses a deposit that delivers less than the
round promises (`ShortDelivery`) — a round that promises more than it holds
pays early claimants and strands the rest. It advances `claimed` **before**
transferring, so a token with a transfer hook cannot re-enter and take the same
index twice.

**Residual.** The vault is unaudited. These paths have been reasoned about and
tested by their author and by nobody else.

## A plan id steers a filesystem path

**Attack.** Craft a plan whose id contains `../` and write outside
`.dedalo/objects`.

**What stops it.** Ids are hex from a hash and are validated as such before
they are used as paths. This is one of the things `tests/adversarial.rs` tries
explicitly.

## Two plans share an id

**Attack.** Construct two different rounds that hash to the same id, so one can
be substituted for the other after review.

**What stops it.** SHA-256, and — more usefully against the practical attack —
**length-prefixed field encoding**, so `("ab","c")` and `("a","bc")` cannot
serialise to the same bytes. `tests/adversarial.rs` tries to build the
collision.

---

## Where the real risk is

Ranked, honestly:

1. **The contract is unaudited and undeployed.** Everything about on-chain
   settlement is unproven in practice, which is why it is not live.
2. **Address substitution.** Procedural, not cryptographic, and the one that
   would actually happen.
3. **Attribution is a policy, not a truth.** It measures merged lines. A
   project that believes that equals contribution will underpay its reviewers
   and its maintainers.
4. **Operator error.** Settling `--amount` instead of `--plan`, skipping a
   range with `--since`, not committing `.dedalo/`.

Nothing in the first four is fixed by more tests on the arithmetic. The
arithmetic is the part that is proved.

## Reporting

Anything where an amount is or could be wrong goes through
[SECURITY.md][sec] as a private advisory, never a public issue — including
credit assigned to the wrong person. See
[Reporting a vulnerability](security.md).

[bits]: ../concepts/identities.md#there-is-no-checksum
[sec]: https://github.com/dedalo-org/dedalo/blob/main/SECURITY.md
