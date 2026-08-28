# JSON Schema for `--json`

`dedalo <command> --json` is a **contract**, not incidental output. `action.yml`
parses it, and [the release policy][releasing] makes removing or renaming a
field a breaking change.

Until now that contract was prose in the handbook plus a test asserting the
fields it happened to check. A consumer building a dashboard, a bot or a
different CI system had to read the prose and hope.

| File | Command |
| --- | --- |
| [`plan.schema.json`](plan.schema.json) | `dedalo plan --json`, and a plan read from `.dedalo/objects` |
| [`attribution.schema.json`](attribution.schema.json) | `dedalo contributors --json` |
| [`status.schema.json`](status.schema.json) | `dedalo status --json` |
| [`verify.schema.json`](verify.schema.json) | `dedalo verify --json` |
| [`proposal.schema.json`](proposal.schema.json) | `dedalo propose --json` |

## Two things these buy that prose does not

**Amounts are strings.** Every amount is a `u128` count of base units
serialised as a decimal string, because JSON numbers are IEEE doubles and lose
precision above 2^53 — well inside the range of a token with 18 decimals. The
schema says `{"type": "string", "pattern": "^[0-9]+$"}`, so a generated client
gets it right without reading a paragraph about it.

**Renaming a field becomes mechanically detectable.** `tests/cli.rs` catches
the fields it happens to assert on; a schema catches all of them. And a schema
diff in a pull request is a readable statement that the contract changed.

## They cannot drift

`tests/json_schema.rs` runs each command against a real repository and
validates its actual output against the schema beside it. A field renamed in
Rust and not here fails the build, which is the only arrangement under which a
published schema is worth anything.

Every schema is `additionalProperties: false`. **Adding** a field is therefore
also a build failure until the schema says so — deliberately, because a field
that appears without being declared is a field no consumer knows to expect and
no reviewer was asked about.

## Versioning

**The version is in the `$id`, and not in the payload.**

```
https://dedalo-org.github.io/dedalo/schema/v1/plan.schema.json
```

The issue that asked for these leaned the other way — a `schema_version` field
in every output, added once while it was cheap. It was not taken, for two
reasons:

- **`PayoutPlan` is stored.** It lives in `.dedalo/objects` and is
  content-addressed; a field describing the *encoding* does not belong inside a
  record that is supposed to be exactly the round. The plan already carries an
  encoding version where it matters — `ENCODING_VERSION` inside
  `PayoutPlan::compute_id`, which makes any change to what the id hashes a
  visible change of id rather than a silent one.
- **It buys less than it looks.** The consumer who would read the field is
  reading output from a version they did not choose — and they already know
  which `dedalo` they invoked, which is the same information.

If a payload version is ever wanted, it belongs on the CLI's own report types
(`StatusReport`, the verify report) which are never stored — **never on
`PayoutPlan`**.

`v1` is the current directory. A breaking change publishes `v2` beside it and
leaves `v1` where it is, because the consumers pinned to it are the reason the
contract exists.

[releasing]: https://dedalo-org.github.io/dedalo/contributing/releasing.html
