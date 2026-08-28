# Branch rulesets

`main.json` is the protection rule set for `main`, which is the only branch
this project protects because it is the only long-lived one. Kept in the
repository so the policy is reviewable rather than remembered.

Work happens on short-lived branches that open a pull request into `main` and
are deleted when it merges. A release is a tag on `main`, not a branch — there
was briefly a second protected branch here, and the arrangement cost more than
it bought.

## The file is authoritative

**When the file and the live ruleset disagree, the file is right and the live
ruleset is the thing that changed.** A protection rule enabled through the web
UI has no author, no reason and no review, and this repository's whole argument
is that things which decide how work lands should have all three.

```sh
scripts/check-ruleset.py           # compare; exits 1 on drift
scripts/check-ruleset.py --write   # adopt live into the file, when deliberate
```

The escape hatch is deliberate and deliberately awkward. Drift and an intended
change look identical from a script, so only a person can say which one it is —
and `--write` produces a diff that has to be committed with a reason, which is
the review the web UI skipped.

GitHub does not apply the file automatically. Import it once, and again after
editing:

**Settings → Rules → Rulesets → New ruleset → Import a ruleset** → select
`main.json`.

## What it enforces

- `main` cannot be **deleted**, **force-pushed**, **created** or **updated**
  outside a pull request.
- **Linear history**: no merge commits on `main`. Squash-only already implies
  it; stating it means a change to the merge methods cannot quietly reintroduce
  one.
- **Signed commits.** Every change lands as a squash commit GitHub creates, and
  GitHub signs those — so this is satisfied by the way work already lands, and
  it refuses anything that arrives another way. Note that it applies to `main`
  only: commits on a feature branch are not signed and are not required to be.
- Every change lands through a **pull request**, squash merged, with one
  approving review and review from a code owner where `CODEOWNERS` applies.
- **Review threads must be resolved** before merging — a payout question left
  hanging is not a merge-blocker by accident, it is one on purpose.
- Two checks must pass, on a branch that is up to date with `main`: `CI` and
  `conventional commit format`. `CI` is the `ci-ok` job, which gates on every
  other job in the workflow, so adding a job does not mean reconfiguring the
  repository.

## Two rules that are deliberately off

Both were enabled at some point without reaching this file, and both blocked
every merge including the documented admin-through-pull-request path.

- **`code_quality`** requires changes to go through a **merge queue**. A merge
  queue is a reasonable thing to want and is not what this repository does: one
  maintainer, squash merges, and an admin bypass that is *through* a pull
  request. With it on, `gh pr merge --squash --admin` refuses on a green pull
  request and there is no path that works.
- **`require_extra_approval_for_unattributed_changes`** requires approval from
  somebody other than the last pusher. With one maintainer there is nobody
  else, so it is unsatisfiable rather than strict — and unlike the ordinary
  review requirement, the pull-request bypass does not honour it.

If a second maintainer joins, both become worth reconsidering, and the
reconsideration should be a change to this file.

## The one deliberate exception

Repository admins may bypass **through a pull request**, not by pushing
directly. The release flow needs this: `.github/workflows/version.yml` opens a
pull request that `.github/workflows/tag.yml` acts on once merged, and both run
as `github-actions[bot]`.

If you would rather have no bypass at all, remove the `bypass_actors` block and
give the release workflows a dedicated app token instead.

## A ruleset is not the only thing that protects a branch

**Classic branch protection is a separate mechanism**, configured elsewhere,
and the two stack. A repository can have both, and GitHub enforces the union —
so a `main` that looks correctly configured here can still be locked by a
classic rule this file knows nothing about.

Check for one before concluding the ruleset is the problem:

```sh
gh api repos/dedalo-org/dedalo/branches/main/protection
```

`404` is the expected answer: this repository's policy is the ruleset, in this
file, and one policy is better than two that can disagree.
