---
description: Audit changes that touch money, attribution or the fee split
allowed-tools: Bash(git diff:*), Bash(git log:*), Bash(cargo test:*), Read, Grep, Glob
---

Audit the current diff for anything that could make Dedalo pay the wrong
amount. Read `CLAUDE.md` for the invariants, then check the changed code
against each one that applies:

- Does any balance, share or fee touch a float?
- Does every split still sum back to exactly its input, including when weights
  are zero, when there is a single payee, and when the amount does not divide
  evenly?
- Do fees still round down, with the remainder going to contributors?
- Is `PayoutPlan::compute_id` still fed by everything that determines the
  outcome — and still free of `created_at` or any other non-deterministic
  input?
- Can a contributor be paid twice, through two identities sharing a wallet?
- Can a contributor be dropped without appearing in `plan.unresolved`?
- Can a plan be settled twice?

For each finding, state the concrete scenario that produces the wrong number —
specific inputs, and what the payout becomes. Skip anything you cannot
demonstrate; a speculative finding in a payments audit is noise.

Finish by running `cargo test --workspace` and reporting whether the existing
invariant tests still pass.
