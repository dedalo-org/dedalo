# Branch rulesets

`main.json` is the protection rule set for the default branch, and
`v0.1.json` the one for the branch the work actually happens on. Both are kept
in the repository so the policy is reviewable rather than remembered.

The two are twins on purpose. `main` carries the `0.0.0` placeholder that holds
the name on crates.io; `v0.1` carries the code. A branch that can be
force-pushed is a branch whose history cannot be trusted, and which of the two
is the default should not decide that.

GitHub does not apply these automatically. Import it once, and again after
editing:

**Settings → Rules → Rulesets → New ruleset → Import a ruleset** → select
the file.

`main.json` targets `~DEFAULT_BRANCH` rather than a name, so it follows the
default branch if it ever moves. `v0.1.json` names `refs/heads/v0.1`, because
that branch is a specific one and not a role.

## What they enforce

- The branch cannot be deleted or force-pushed.
- Every change lands through a pull request, squash merged, with one approving
  review and review from a code owner where `CODEOWNERS` applies.
- Review threads must be resolved before merging — a payout question left
  hanging is not a merge-blocker by accident, it is one on purpose.
- Two checks must pass, on a branch that is up to date with the base:
  `CI` and `conventional commit format`. `CI` is the `ci-ok` job, which gates
  on every other job in the workflow, so adding a job does not mean
  reconfiguring the repository.

## The one deliberate exception

Repository admins may bypass **through a pull request**, not by pushing
directly. The release flow needs this: `.github/workflows/version.yml` opens a
pull request that `.github/workflows/tag.yml` acts on once merged, and both run
as `github-actions[bot]`.

If you would rather have no bypass at all, remove the `bypass_actors` block and
give the release workflows a dedicated app token instead.
