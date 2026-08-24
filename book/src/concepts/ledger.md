# The ledger

`.dedalo/` is shaped like `.git`, because the problem is the same one: a record
that many people must be able to check, living in the project rather than on
someone's server.

```text
.dedalo/
├── HEAD                        ref: refs/ledger/main
├── refs/ledger/main            the newest entry's id
└── objects/de/dc6ddbbe….json   one file per entry and per plan
```

## Why a chain

An append-only file is append-only **by convention**. Nothing stops someone
editing a line, and nothing afterwards can tell. For a record of who was paid
what, that is not a foundation.

So each entry names its parent, and an entry's id is a hash over its parent's
id together with its own contents:

```text
HEAD ─▶ dedc9f… ──parent──▶ dedc41… ──parent──▶ dedc07… (root)
        settled            settled             plan-created
```

Change anything in an old entry and its id changes. Every entry after it named
the old id, so their ids change too, and `HEAD` stops matching. **One value
attests to the whole payout history.** This is append-only by arithmetic rather
than by convention.

Publish `HEAD` — in a release note, a README, a tweet — and anyone with a clone
can confirm that what they are reading is what was written.

## Verifying it

```console
$ dedalo verify
head dedc6ddbbef5415e6dcbf805b60affd83c49
ok 4 entries hash to their recorded ids
ok 2 settled plans present and self-consistent
```

`verify` recomputes every id from the entry it came from and walks the chain to
the root. It reads only what is committed to the repository: **no network, no
key, no credentials**. That is the property that matters. A check that requires
the maintainer's cooperation is a check the maintainer is being trusted for; a
contributor, an auditor or a funder can run this on a fresh clone and get an
answer that does not depend on anybody's word.

Exit code is non-zero if the chain does not verify, so it belongs in CI.

## Idempotence

The ledger is what makes a round happen once.

Before settling, Dedalo checks whether the plan's id is already recorded as
settled, and refuses if it is. Because a plan is [content-addressed](plans.md),
"the same round" is an exact notion rather than a guess about dates and
amounts.

While settling, it holds an **exclusive lock** (`.dedalo/settle.lock`), so two
concurrent jobs cannot both pass the check and both proceed. A retried CI job
does not pay twice, and neither does a workflow that somehow started twice.

The same guarantee is enforced a second time, independently, on chain:
`DedaloClaim.deposit` refuses a plan id it has already seen. Two mechanisms for
one rule, because the failure mode is paying people twice out of a treasury.

## Why plain JSON

Objects are stored as readable JSON rather than compressed blobs. A round is
meant to be reviewable in a pull request, and a zlib blob is not reviewable —
the diff would be noise, and the review would be of the tool rather than of the
numbers.

The cost is size. It is the right trade: a project running twelve rounds a year
accumulates kilobytes, and being able to read a payout record in a pull request
is worth more than the kilobytes.

## Why not in `.git/`

Because it has to be committed.

A CI job clones fresh. Anything in `.git/` that is not a commit does not
survive that clone, and a runner that cannot see past rounds would compute the
range from the beginning of history and pay every one of them again.

> **Careful** — `.dedalo/` and `dedalo.toml` are public records and belong in
> git. Do not add them to `.gitignore`. If a round is missing from a clone,
> the next round will overlap it.

## Entry kinds

| Kind | Written when | Carries |
| --- | --- | --- |
| `plan-created` | `plan --save` | the plan id and its gross amount |
| `settled` | a settlement completes | the plan id and the backend's receipt |

The pending range is derived from the newest `settled` entry: `scan` starts
after the commit that round covered. `--since` overrides it when you need to
recompute a range by hand.

## Migrating an old ledger

Repositories written by a pre-chain version have a flat `ledger.jsonl`. It is
detected rather than ignored, because silently starting a new chain next to an
old log would lose the history that says which rounds already happened:

```bash
dedalo ledger --migrate
```

converts the flat log into chain entries and stops. Commit the result.
