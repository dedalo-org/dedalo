# 0005 — The history layer is git, and says so

**Status.** Accepted. Extracted from `docs/settlement-architecture.md`.

## Context

Stage 1 of the pipeline reads merges out of a repository. The vocabulary says
so everywhere: `git::`, `GitBackend`, `[git]` in the config, `Error::Git`.

The tempting refactor is that a payout should derive from "a history nobody can
quietly rewrite", with git as one instance — and that naming git throughout is
an implementation leaking into the domain language.

## Decision

**The history layer is git, the code says git, and that is deliberate.**

`GitBackend` stays a trait, because substituting it in tests is worth the
indirection on its own. What it stops carrying is the implication that a second
backend is coming.

## What was rejected, and why

**Generalising to "a version control system".** The abstraction is real —
everything downstream of `GitBackend` sees `MergeEvent` values and never a git
invocation, and the tests already substitute another implementation. What is
not real is the second implementation.

Jujutsu, Sapling, Pijul and Fossil each have something that means "this change
landed", and **they do not agree that it is a commit with two parents**. So
`MergeEvent`, first-parent diffing and a revision syntax would all have to
become negotiable. That is a redesign of the one part of the pipeline that
decides who is owed what, paid for now, on behalf of a user who does not exist.

Renaming without generalising — `HistoryBackend` over a git-shaped interface —
was rejected as worse than either option: it claims a generality the types do
not have, and the next person has to read the implementation to find out.

## Consequences

- **The naming is honest**, and a reader who sees `git` can assume git.
- **Adding a second VCS is a redesign, not a plug-in**, and this record is what
  that redesign has to argue with.
- **The real gap is inside git, not beyond it.** Attribution finds nothing in a
  squash-merge repository, and this project's own `main` is squash-only. That
  is a defect affecting real users today, and it is where the effort belongs.

## Related

- [0003](0003-solana-and-the-address-layer.md) — the same reasoning at the other
  end of the pipeline: name what is true, and let the compiler point at the
  edits when it stops being true.
