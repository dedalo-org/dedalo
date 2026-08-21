# Releasing Dedalo

A release is one version number, one tag, and one set of artifacts. Both crates
in the workspace share that version, so `v0.4.0` means `dedalo` **and**
`dedalo-core` are at `0.4.0`.

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

## Cutting a release

1. **Open the release pull request.** Run the **Version** workflow from the
   Actions tab and pick `patch`, `minor`, `major`, or an explicit version. It
   bumps the workspace version, refreshes `Cargo.lock`, prepends the generated
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
   - publishes `dedalo-core` then `dedalo` to crates.io;
   - pushes `ghcr.io/4137314/dedalo:<version>` and `:latest`.

If step 4 fails after the tag exists, fix forward: delete the tag, merge the
fix, and re-run **Tag**. Never move a tag that a release already published.

### The one tag that does move

`v0` — and `v1` after that — follows the latest release, because
`uses: 4137314/dedalo@v0` is how a GitHub Action is consumed. It is a pointer
to an immutable release tag, so nothing that was published ever changes
underneath someone. Pin `@v0.1.0` instead if you want the workflow frozen.

## Required repository configuration

| Secret / setting | Where | Used by |
| --- | --- | --- |
| `CARGO_REGISTRY_TOKEN` | environment `crates-io` | publishing to crates.io |
| Pages source: *GitHub Actions* | Settings → Pages | the project site and API docs |
| `contents: write` for Actions | Settings → Actions | tagging and release creation |

`GITHUB_TOKEN` covers the GitHub release and the container registry; no extra
secret is needed for either.

## Verifying a release

Anyone can check that a published binary matches what the tag says:

```bash
# The checksum published beside it
curl -fsSL https://github.com/4137314/dedalo/releases/download/v0.1.0/dedalo-v0.1.0-x86_64-unknown-linux-gnu.tar.gz.sha256
sha256sum dedalo-v0.1.0-x86_64-unknown-linux-gnu.tar.gz

# Or the signed provenance, which also proves which workflow built it
gh attestation verify dedalo-v0.1.0-x86_64-unknown-linux-gnu.tar.gz --repo 4137314/dedalo
```

Or rebuild it from source with the pinned toolchain:

```bash
nix build github:4137314/dedalo/v0.1.0
```
