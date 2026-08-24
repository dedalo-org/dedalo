# Attribution

Attribution answers one question: **of the work merged in this range, what
fraction is each person's?** It answers it in integers, and it never looks at
money.

## The unit

Scores are **milli-points**: `u128`, where 1 point is 1,000 milli-points.

That is not a style choice. `points_per_insertion = 1.0` is a decimal in the
config because writing "half a point per deleted line" as `0.5` is what people
mean, but a float in the scoring path would make the result depend on the order
of additions and on the machine's rounding mode. Two contributors could get
different shares from the same history on different laptops, and neither could
prove the other wrong. So the decimals in the config are converted to
milli-points once, at the edge, and everything after that is integer
arithmetic.

## What a merge is worth

```text
merge_points = base_points
             + insertions × points_per_insertion
             + deletions  × points_per_deletion
             ── capped at max_points_per_merge
```

| Key | Default | What it does |
| --- | --- | --- |
| `base_points` | `100` | Flat score every merged pull request earns, regardless of size. |
| `points_per_insertion` | `1.0` | Per added line. |
| `points_per_deletion` | `0.5` | Per removed line. Deleting code is work too. |
| `max_points_per_merge` | `5000` | Ceiling, so one merge cannot dominate a round. |
| `credit_merger` | `false` | Also credit whoever pressed merge. |
| `split_with_co_authors` | `true` | Share a commit's score with its `Co-authored-by:` trailers. |

The diff is measured **against the merge's first parent**, which is what "what
did this merge bring into main" means. A merge that brings in nothing scores
`base_points` and no more.

### Why `base_points` exists

Without it, scoring is purely per-line, and per-line scoring rewards verbosity.
A one-line fix to an off-by-one that was losing money is worth more than a
three-hundred-line refactor of a test helper, and no line-counting formula will
ever say so. `base_points` is the part of the score that says "this was
reviewed and merged", which is the only judgement git actually records.

Projects that want the flat part to dominate raise it; projects paying for bulk
work lower it. There is no correct value, and the config is the place that
decision is written down and reviewed.

### Why the cap exists

One merge that vendors a dependency, regenerates a lockfile, or imports a
grammar can be a hundred thousand lines. Without `max_points_per_merge` that
merge takes the round. The cap is applied to the merge before its score is
split between people, so it cannot be evaded by adding co-authors.

## Splitting within a merge

A merge's points are divided across the people it credits:

- every commit's **author**;
- their **`Co-authored-by:` trailers**, when `split_with_co_authors` is on;
- whoever pressed merge, when `credit_merger` is on.

Splitting uses the same largest-remainder method the [money](money.md#splitting)
does, so a merge's points sum back to exactly what the merge was worth. There
is no path where a rounding step quietly creates or destroys a point.

## Who is excluded

```toml
[git]
ignore_subjects = ["chore(release)", "Merge branch"]
ignore_emails   = ["noreply@github.com", "actions@github.com"]
```

`ignore_subjects` drops a merge entirely when its subject starts with one of
these. Release commits are the main case: a version bump merged by automation
is not contribution, and paying for it means paying for the act of paying.

`ignore_emails` drops an author. Bots commit, and a bot with a wallet is a way
for a round to leak.

Both are prefix and exact matches respectively — no globbing, no regular
expressions. A pattern language here would be a place for a subtle mistake to
hide, and the thing being decided is who gets paid.

## What attribution does not see

Worth stating plainly, because it is the honest limit of the model:

- **Review.** The person who caught the bug in review scores nothing today.
  This is the largest known gap; [review-weighted attribution][reviews] is
  tracked and on the roadmap.
- **Issues, triage, support.** A maintainer who spends the month answering
  questions merges nothing and earns nothing.
- **Design and decisions.** The conversation that avoided a month of work
  leaves no merge.
- **Squash-only repositories.** A repository that squash-merges without merge
  commits currently produces no merge events at all — see
  [issue #13][squash].

None of these is a reason not to run Dedalo. They are reasons to know what the
number means: it is a share of *merged code*, not a share of *contribution*,
and a project that wants to reward the rest can do so from the treasury slice
that every round sets aside.

## Determinism

The same range and the same policy always produce the same weights. Merges are
ordered oldest to newest by the backend, scores are integers, and the split is
deterministic. Nothing consults a clock, a random number, an environment
variable or the network.

That property is what makes `plan` reproducible, and it is tested rather than
asserted: `verification.toml` records `attribution` as covered by property
tests, and `tests/adversarial.rs` asks specifically whether two different
histories can be made to produce the same weights, or one history two different
sets.

[reviews]: https://github.com/dedalo-org/dedalo/issues/12
[squash]: https://github.com/dedalo-org/dedalo/issues/13
