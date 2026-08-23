# Contributing to Dedalo

Dedalo pays contributors for merged code. Contributing here is, fittingly,
the thing the project is built to reward.

## Getting a development environment

Install `rustup`. It reads `rust-toolchain.toml` when you enter the
repository, so you get the same compiler CI uses without choosing one.

## The loop

```bash
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all
```

CI additionally builds the declared MSRV, rustdoc with `-D warnings`,
coverage, the musl packaging path, and public-API compatibility. Running the
three commands above catches almost everything before it gets there.

## Branches and commit messages

`main` is always releasable, and every change lands through a pull request.
Work on short-lived branches named for what they do — `feat/…`, `fix/…`,
`docs/…`, `ci/…` — and delete them once merged.

Pull request titles follow [Conventional Commits](https://www.conventionalcommits.org)
and are checked automatically. Because pull requests are squash merged, **the
title becomes the changelog entry**, so write it for a reader of the release
notes:

```
feat(cli): add `dedalo identity export`
fix(money): keep dust with contributors when a weight is zero
docs: explain how the protocol fee funds the network
```

Anything that changes what people are paid — amounts, plan ids, the fee split —
is a breaking change even when it compiles. Say so with `BREAKING CHANGE:` in
the body. [RELEASING.md](RELEASING.md) has the full policy.

## What a good pull request looks like

- **One concern per PR.** A refactor and a behaviour change in the same diff
  are two PRs.
- **Public API changes carry rustdoc.** The crate sets
  `#![warn(missing_docs)]` and CI builds docs with `-D warnings`, so an
  undocumented `pub` item is a build failure.
- **A new module carries a verification entry.** `verification.toml` accounts
  for every module under `src/`, and `tests/verification_manifest.rs` fails the
  build if one is missing. Say how it is verified, or why it needs none — an
  exemption with a reason is a fine answer, and the gate checks the reason
  stays true by refusing to let an exempt module do arithmetic or build an
  address. Adding arithmetic anywhere changes a recorded count and fails the
  build until someone looks.
- **Money changes carry tests.** Anything touching `money`, `attribution`,
  `treasury` or `payout` needs a test proving the amounts still balance —
  including the awkward cases: zero weights, a single payee, amounts that do
  not divide evenly. A new rule about what people are paid belongs in
  `tests/properties.rs`, where generated inputs will try to
  break it, not only in one example you chose.
- **CLI output changes carry tests.** `action.yml` parses `--json`; renaming a
  field breaks it silently. `tests/cli.rs` is what catches
  that.
- **Commit messages say why.** The subject is the change; the body is the
  reason.
- **Co-authors get credit.** Use `Co-authored-by:` trailers — Dedalo reads
  them, and they are what splits a payout between pair partners.

## Where things live

`src/lib.rs` documents the pipeline and exposes `Engine`,
the shortest path through all four stages. From there: `money.rs` (the
arithmetic), `payout.rs` (the artifact), `treasury.rs` (the fee split). The
invariants the project guarantees are stated as property tests in
`tests/properties.rs` — read those before changing anything
that decides an amount.

## Getting paid

Once the project's own funding rounds are live, contributions here earn a
share. Add yourself with:

```bash
dedalo identity link <your-handle> <your-wallet> --email <your-git-email>
```

and include it in your first pull request.

## Reporting security issues

Do not open a public issue. See [SECURITY.md](SECURITY.md).
