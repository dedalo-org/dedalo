# Branch rulesets

`main.json` is the protection rule set for the default branch, kept in the
repository so the policy is reviewable rather than remembered.

GitHub does not apply these automatically. Import it once, and again after
editing:

**Settings → Rules → Rulesets → New ruleset → Import a ruleset** → select
`main.json`.

## What it enforces

- `main` cannot be deleted or force-pushed.
- Every change lands through a pull request, squash merged, with one approving
  review and review from a code owner where `CODEOWNERS` applies.
- Review threads must be resolved before merging — a payout question left
  hanging is not a merge-blocker by accident, it is one on purpose.
- Three checks must pass, on a branch that is up to date with `main`:
  `CI`, `nix flake check / ubuntu-latest`, and `conventional commit format`.

## The one deliberate exception

Repository admins may bypass **through a pull request**, not by pushing
directly. The release flow needs this: `.github/workflows/version.yml` opens a
pull request that `.github/workflows/tag.yml` acts on once merged, and both run
as `github-actions[bot]`.

If you would rather have no bypass at all, remove the `bypass_actors` block and
give the release workflows a dedicated app token instead.
