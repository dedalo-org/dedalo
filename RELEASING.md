# Releasing Dedalo

A release is one version number, one tag, and one set of artifacts. The
library and the binary are one crate, so `v0.0.1` will mean exactly one thing:
`dedalo` is at `0.0.1` on crates.io, and the tagged commit built it.

Today `dedalo` is at `0.0.0` — a placeholder that holds the name and contains
no code. `0.0.1` is the first release that carries any.

Nothing here requires a maintainer to run commands locally: the whole flow is
two workflow runs and one pull request review.

## Versioning policy

Semantic versioning, with the pre-1.0 caveat that minor bumps may break the
API. What counts as breaking is deliberately wider than usual, because people
are paid based on this code:

| Change | Bump |
| --- | --- |
| A given history + config produces different payout **amounts** | major |
| A plan's `id` changes for unchanged inputs | major |
| Any public Rust API is removed or changes shape | major (minor pre-1.0) |
| A CLI flag or JSON output field is removed or renamed | major (minor pre-1.0) |
| New attribution options, backends, commands, output fields | minor |
| Bug fixes that make amounts *correct*, docs, internals | patch |

A change to `Amount::split_by_weights`, `PayoutPlan::compute_id`, or the fee
split is a breaking change even if it compiles, because it changes what people
receive. Say so in the pull request body with `BREAKING CHANGE:`.

## The minimum supported Rust version

The MSRV is **1.90.0**. It is enforced rather than documented: CI's `msrv` job
and `ws-check` both build with exactly the compiler `rust-version` names, so it
cannot drift upward by accident.

What was missing was the policy — under what circumstances the floor moves, and
what a consumer pinned to an older compiler can expect.

### Raising it is a minor bump

Pre-1.0, an MSRV bump is a **minor** version bump. After 1.0 it becomes a
**major** one.

That is stricter than treating it as a patch, and it is the honest reading: a
consumer who cannot build the new version has had something taken away, and the
version number should say so.

It is also less dramatic than it used to be. **Cargo respects `rust-version`
during resolution**, so a consumer on an older toolchain resolves to the newest
version their compiler can build rather than getting a wall of type errors.
A bump narrows what is available to them; it does not break their build.

### How far back the floor stays

**The current stable release, minus two.** When Rust ships 1.93, the floor may
move to 1.91 and no further.

Two alternatives were considered and rejected:

- **"Whatever Debian stable ships"** — defensible, and it makes the floor a
  function of somebody else's freeze schedule. Debian stable can be two years
  behind, which in practice means never raising the MSRV, including for a
  security fix.
- **"Latest stable"** — no floor at all, which makes `rust-version` decoration.

N−2 is roughly a four-month window. It is short enough that a security fix is
reachable and long enough that a distribution's toolchain is usually inside it.

### What justifies a bump

In descending order of strength:

1. **A security fix that is unavailable otherwise.** This is what happened: the
   fix for RUSTSEC-2026-0220 in `ruint` needs 1.90, `alloy-sol-types` needs
   `ruint`, and the alternative was shipping a known-vulnerable big-integer
   library **in a payments tool**. That is not a close call.
2. **A language or library feature that removes a real hazard** — something
   that lets unsafe code, a hand-rolled invariant, or a panic path be deleted.
   Convenience is not this.
3. **A dependency that merely prefers a newer compiler.** This is *not*
   sufficient on its own. Pin the dependency back and revisit; a transitive
   crate's taste is not a reason to narrow who can build a payout tool.

The difference between (1) and (3) is the whole point of writing this down, so
that the next bump is argued rather than absorbed.

### Three places move together

An MSRV bump touches **three** files, and the gate only catches two of them:

| File | What reads it |
| --- | --- |
| `Cargo.toml` — `rust-version` | cargo resolution, CI's `msrv` job, docs.rs |
| `rust-toolchain.toml` — `channel` | rustup, on entry to the repository |
| the workspace flake's Rust pin | `nix develop`, and therefore `ws-check` |

`ws-check` fails if the flake's pin and `Cargo.toml`'s `rust-version` disagree.
Nothing checks `rust-toolchain.toml` against either, because it names the
*development* toolchain rather than the floor — those are allowed to differ, and
usually do. Change it anyway, or contributors build with a compiler CI does not
use.

A bump gets a changelog entry saying which of the three reasons above applies.

## Branching

- `main` is always releasable. Every change lands through a pull request.
- Work happens on short-lived branches: `feat/…`, `fix/…`, `docs/…`, `ci/…`.
- No long-lived release branches. A patch for an old version, if ever needed,
  branches from its tag as `release/0.3.x`.

## Commit and pull request titles

Titles follow [Conventional Commits](https://www.conventionalcommits.org) and
are checked by `.github/workflows/pr-title.yml`. Pull requests are squash
merged, so **the pull request title becomes the changelog entry**:

```
feat(cli): add `dedalo identity export`
fix(money): keep dust with contributors when a weight is zero
refactor(git): move trailer parsing behind the backend trait
```

## Cadence

Releases are cut when a milestone closes, not on a calendar. The dates below
are **targets attached to the milestones**, so the schedule and the work are
one thing rather than two that drift:

| Version | Milestone | Target | What makes it releasable |
| --- | --- | --- | --- |
| `0.1.0` | [first release](https://github.com/dedalo-org/dedalo/milestone/1) | 30 Sep 2026 | The crate on crates.io, the reference on docs.rs, the handbook published. Not new capability — availability. |
| `0.2.0` | [on-chain settlement](https://github.com/dedalo-org/dedalo/milestone/2) | 31 Jan 2027 | A published independent audit, a deployed claim contract, a multisig with signers who are not one person, and one testnet round settled end to end. |
| `0.3.0` | [attribution beyond lines](https://github.com/dedalo-org/dedalo/milestone/4) | 30 Apr 2027 | Review-weighted attribution, and a history layer that is not git-shaped. Everything in it changes what people are paid, so all of it is breaking. |
| `1.0.0` | [a stable promise](https://github.com/dedalo-org/dedalo/milestone/5) | 30 Sep 2027 | At least one project other than this one having run real rounds through it. |

**Patch releases are cut on demand**, whenever a fix matters more than waiting —
and any fix to an amount matters more than waiting. They do not need a
milestone.

Three things this cadence is deliberately not:

- **Not time-boxed.** A milestone that is not ready slips. Shipping `0.2.0` on
  a date with an unaudited contract would be shipping the date, and the date is
  not what anybody is trusting.
- **Not a promise of the contents.** An issue can leave a milestone. What the
  table promises is the *condition*, in the last column, not the issue list.
- **Not a reason to hold a fix.** Nothing waits for a milestone.

If a target slips, move the milestone's due date and say why in the milestone
description. A silently overdue milestone is worse than a moved one, because it
teaches people the dates mean nothing.

## Cutting a release

1. **Open the release pull request.** Run the **Version** workflow from the
   Actions tab and pick `patch`, `minor`, `major`, or an explicit version. It
   bumps the crate version, refreshes `Cargo.lock`, prepends the generated
   changelog section, and opens a pull request labelled `release`.

2. **Review it.** Read the changelog diff as a user would: does it describe
   what changed, and is the bump right for it? Edit the changelog in the
   branch if a generated line is unclear — the file, not the workflow, is the
   published record.

3. **Merge it.** Merging a `release`-labelled pull request triggers the **Tag**
   workflow, which creates `v<version>` through the GitHub API — no credential
   is ever written into a checkout — and then calls the release workflow
   directly.

   It has to call it rather than rely on the tag: GitHub suppresses events
   caused by `GITHUB_TOKEN`, so a tag created by a workflow raises no `push`.
   A tag pushed by a person still triggers **Release** on its own.

4. **Watch the release build.** The **Release** workflow then:
   - re-runs fmt, clippy and the full test suite on the tagged commit;
   - verifies the tag matches the workspace version;
   - builds binaries for five targets with SHA-256 checksums;
   - attaches signed build provenance to every archive;
   - publishes a GitHub release with the changelog section as its notes;
   - publishes `dedalo` to crates.io, unless that version is already there —
     a re-run of a release must not fail on the one step that cannot repeat;

If step 4 fails after the tag exists, fix forward: delete the tag, merge the
fix, and re-run **Tag**. Never move a tag that a release already published.

### The one tag that does move

`v0` — and `v1` after that — follows the latest release, because
`uses: dedalo-org/dedalo@v0` is how a GitHub Action is consumed. It is a pointer
to an immutable release tag, so nothing that was published ever changes
underneath someone. Pin `@v0.0.1` instead if you want the workflow frozen.

## Required repository configuration

| Secret / setting | Where | Used by |
| --- | --- | --- |
| **Trusted Publishing** | crates.io → dedalo → Settings → Trusted Publishing | publishing to crates.io — see below |
| Pages source: *GitHub Actions* | Settings → Pages | the handbook at `/dedalo/` |
| `contents: write` for Actions | Settings → Actions | tagging and release creation |
| `main` ruleset, imported from `.github/rulesets/main.json` | Settings → Rules | squash-only merges, required checks, no force-push |
| Squash title *PR title*, message *PR body* | Settings → General | the changelog: `git-cliff` reads the commit subject, and `BREAKING CHANGE:` is declared in the pull request body |

`GITHUB_TOKEN` covers the GitHub release and the container registry; no extra
secret is needed for either.

### There is no registry token, on purpose

Publishing does not use a stored `CARGO_REGISTRY_TOKEN`. It uses **Trusted
Publishing**: crates.io mints a token that lives thirty minutes, in exchange
for a signed OIDC claim GitHub issues at run time, and only for this repository
running this workflow in this environment.

A long-lived registry token is a credential that sits in a settings page until
somebody leaks it, and it is exactly the thing
[Decision 2](docs/settlement-architecture.md) refuses for the money path. The
same argument applies to the thing that ships the code, and this is now the
industry default rather than a preference of ours.

Configure it once, on crates.io, under **dedalo → Settings → Trusted
Publishing → Add**:

| Field | Value |
| --- | --- |
| Repository owner | `dedalo-org` |
| Repository name | `dedalo` |
| Workflow filename | `release.yml` |
| Environment | `crates-io` |

The environment matters: it is what stops a workflow run from another branch,
or a job that is not this one, from being able to publish at all.

Until that entry exists, the `crates-io` job fails at the token exchange with a
message naming the missing configuration — which is the right failure. Nothing
else in a release depends on it, so the tag, the binaries and the GitHub
release all still happen.

The API reference needs nothing here: docs.rs builds it from the crate the
release publishes.

> The two squash settings are load-bearing rather than cosmetic. With the
> default *commit or PR title*, a single-commit pull request takes its subject
> from the commit rather than from the title the `pr-title` check validated —
> so an unconventional subject reaches the changelog unchecked. With the
> default *commit messages*, the pull request body never reaches the commit,
> and a `BREAKING CHANGE:` declared there is silently lost from both the
> changelog and the version bump.

## Verifying a release

Anyone can check that a published binary matches what the tag says:

```bash
# The checksum published beside it
curl -fsSL https://github.com/dedalo-org/dedalo/releases/download/v0.0.1/dedalo-v0.0.1-x86_64-unknown-linux-gnu.tar.gz.sha256
sha256sum dedalo-v0.0.1-x86_64-unknown-linux-gnu.tar.gz

# Or the signed provenance, which also proves which workflow built it
gh attestation verify dedalo-v0.0.1-x86_64-unknown-linux-gnu.tar.gz --repo dedalo-org/dedalo
```

Or rebuild it from source with the pinned toolchain:

```bash
cargo install dedalo --locked --version 0.1.0
```
