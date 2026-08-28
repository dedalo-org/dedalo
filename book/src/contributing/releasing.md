# Releasing

One version, one tag, one set of artifacts. The library and the binary are one
crate, so `v0.4.0` means exactly one thing: `dedalo` is at `0.4.0` on
crates.io, and the tagged commit built it.

The full policy is [RELEASING.md][releasing] in the repository. This chapter is
the part that affects anyone writing a pull request.

## What counts as breaking

Deliberately wider than usual, because people are paid based on this code:

| Change | Bump |
| --- | --- |
| A given history + config produces different payout **amounts** | major |
| A plan's **`id` changes** for unchanged inputs | major |
| Any public Rust API is removed or changes shape | major (minor pre-1.0) |
| A CLI flag or a `--json` field is removed or renamed | major (minor pre-1.0) |
| New attribution options, backends, commands, output fields | minor |
| Bug fixes that make amounts *correct*, docs, internals | patch |

A change to `Amount::split_by_weights`, `PayoutPlan::compute_id`, or the fee
split is **breaking even if it compiles**, because it changes what people
receive. Say so with `BREAKING CHANGE:` in the pull request body.

> **Money** — a plan id that changes for unchanged inputs invalidates every
> published round's reproducibility check. The id encoding carries a version
> byte precisely so an old id and a new one are distinguishable rather than
> merely different.

## The changelog is your pull request title

`CHANGELOG.md` is generated from Conventional Commit subjects with `git-cliff`,
and pull requests are squash merged. **The title becomes the release note.**

```text
feat(cli): add `dedalo identity export`
fix(money): keep dust with contributors when a weight is zero
```

Write it for somebody reading the release notes, not for somebody reading the
diff.

## Cutting one

Nothing requires a maintainer to run commands locally: two workflow runs and
one pull request review.

1. **Open the release pull request** — run the **Version** workflow from the
   Actions tab and pick `patch`, `minor`, `major` or an explicit version. It
   bumps the crate version, refreshes `Cargo.lock`, prepends the generated
   changelog section, and opens a pull request labelled `release`.

2. **Review it** — read the changelog diff as a user would. Does it describe
   what changed, and is the bump right for it? Edit the changelog in the branch
   if a generated line is unclear: the file, not the workflow, is the published
   record.

3. **Merge it** — merging a `release`-labelled pull request triggers **Tag**,
   which creates `v<version>` through the GitHub API (no credential is ever
   written into a checkout) and then calls **Release** directly. It has to call
   it: GitHub suppresses events caused by `GITHUB_TOKEN`, so a tag created by a
   workflow raises no `push`.

4. **Watch the release build** — it re-runs fmt, clippy and the full suite on
   the tagged commit, verifies the tag matches the workspace version, builds
   five targets with SHA-256 checksums, attaches signed provenance, publishes
   the GitHub release with the changelog section as its notes, and publishes to
   crates.io.

> **Careful** — never edit the version by hand. `scripts/bump-version.sh` is
> the only thing allowed to change it, and the **Version** workflow drives it.
> It writes three places: `Cargo.toml`, `Cargo.lock`, and `CITATION.cff` — a
> citation naming a version that does not exist renders perfectly, which is why
> it is the one most likely to rot.

If step 4 fails after the tag exists, **fix forward**: delete the tag, merge the
fix, re-run **Tag**. Never move a tag a release already published — somebody may
already have downloaded it.

## The one tag that moves

`v0` — and `v1` after it — follows the latest release, because
`uses: dedalo-org/dedalo@v0` is how a GitHub Action is consumed. It is a pointer
to an immutable release tag, so nothing that was published ever changes
underneath anybody. Pin `@v0.0.1` to freeze a workflow.

## Verifying a release

Anyone can check a published binary against what the tag claims:

```bash
curl -fsSL https://github.com/dedalo-org/dedalo/releases/download/v0.0.1/dedalo-v0.0.1-x86_64-unknown-linux-gnu.tar.gz.sha256
sha256sum dedalo-v0.0.1-x86_64-unknown-linux-gnu.tar.gz

# Or the signed provenance, which also proves which workflow built it
gh attestation verify dedalo-v0.0.1-x86_64-unknown-linux-gnu.tar.gz --repo dedalo-org/dedalo
```

Or rebuild it:

```bash
cargo install dedalo --locked --version 0.1.0
```

## Where documentation goes at release time

| Documentation | Published by | Versioned? |
| --- | --- | --- |
| **API reference** | docs.rs, from the crates.io upload | **yes, per release** |
| **This handbook** | GitHub Pages, from `main` | no — always current |
| Changelog | The GitHub release and `CHANGELOG.md` | per release |

The API reference is not built from `main` any more. docs.rs builds it from the
published crate, which means the reference somebody reads matches the version
they installed rather than whatever `main` looked like that morning.

[releasing]: https://github.com/dedalo-org/dedalo/blob/main/RELEASING.md
