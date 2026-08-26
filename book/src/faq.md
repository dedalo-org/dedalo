# FAQ

### Does Dedalo hold my money?

No, and it cannot. It holds **no signing key** — not in CI, not in config, not
on a maintainer's machine. `dedalo propose` prints transactions; people execute
them from a multisig. There is no flag that changes this.

### Can I use it without any crypto?

Yes, for everything except settlement. `plan`, `contributors`, `scan`, `verify`
and the whole ledger work offline with no chain involved. Plenty of projects
will get value from "who contributed what, computed the same way every time"
without ever funding a round.

The `[wallets]` addresses are required by the config, but the zero-address
placeholders are fine if you never settle.

### Is it live? Can it pay people today?

The pipeline is live. **On-chain broadcasting is not.** The `solana` backend
validates the config, builds the exact call a plan translates into, and then
returns `NotImplemented` rather than a fake receipt. The claim contract is
unaudited and undeployed.

See [Before real funds move](operating/multisig.md#before-real-funds-move).

### Why does the same command give a different plan id?

Something that goes into the id changed: a new merge landed, or `dedalo.toml`
changed. The id covers the project, asset, range, split and items — not the
timestamp. `git diff dedalo.toml` and compare `range.to_commit`.

### Why is a contributor missing from my plan?

Almost always because no identity links their email. Check:

```bash
dedalo identity missing
```

They are not dropped — they are in `plan.unresolved`, and their share is in
`undistributed`.

### Someone commits from three different emails. Do they get paid three times?

No. One handle, one wallet, many emails, and contributors are merged into a
single item before the plan is finalised. Addresses compare case-insensitively,
so the two EIP-55 spellings of one account are also one payee.

### Does it work with squash merges?

Not today, if your repository squash-merges without creating merge commits.
Attribution reads merge commits, and a squash-only history has none — which
currently produces an *empty round* rather than an error. That is
[issue #13][squash], and it is a real gap.

### What about rebase merges?

Same problem, same issue.

### Why not just count commits?

Because a commit is not a decision. A merge is the moment a project has already
decided the work was worth having — reviewed, dated, attributed. Counting
commits pays for typing.

### Does it pay reviewers?

Not yet, and this is the largest known gap in the model.
[Review-weighted attribution][reviews] is tracked. Until then, a project that
wants to reward review can do it from the treasury slice every round sets
aside.

### Can a contributor game the scoring?

Partly, and the honest answer is in the [threat model](trust/threat-model.md).
`max_points_per_merge` caps any single merge. Nothing prevents verbose code —
but the merge had to be reviewed, and a project merging padding has a review
problem rather than an attribution problem.

### Why basis points instead of percentages?

Because a percentage invites a decimal, and a decimal in a money path invites a
float. Basis points are `u16` integers: 10,000 = 100%, and every value of them
is proved to round down.

### Why is `.dedalo/` committed? Isn't that noise in my diffs?

It has to be committed, or a CI job that clones fresh cannot see past rounds and
would pay them again. The objects are plain JSON rather than compressed blobs
so a round is **reviewable in a pull request** — which is most of the value of
having them in the repository at all.

### Can I edit `.dedalo/` to fix a mistake?

No. Editing an entry breaks its hash, and every entry after it. That is the
mechanism working. If a round was wrong, the fix is a new round, and the record
of the wrong one stays — that is what a ledger is.

### What happens to money for someone who never links a wallet?

Under the [pull model](operating/multisig.md), it stays in the round against
the Merkle root until they claim it, or until the 180-day claim window closes
and the depositor sweeps what is left. It is not silently redistributed and it
is not sent to the treasury.

### Why 180 days, and why can't I change it?

Fixed rather than chosen by the depositor, because a depositor who could choose
the window could choose one that closes before anybody claims.

### Which chain does it use?

Undecided, and that is deliberate. The template names Base and mainnet USDC — a
default that was never chosen on purpose and should be before anyone
broadcasts. [Issue #15][chain]. The address layer knows about address
*formats*, not one chain.

### Is there a token?

No. The protocol fee flows to an Open Collective wallet. There is nothing to
buy.

### Is there a hosted version?

No, and there is not planned to be one. Dedalo runs in your pipeline and reads
your repository. A dashboard that holds the data is the thing this project
exists not to be.

### How do I check that a project's published rounds are real?

Clone it and run `dedalo verify`, then recompute a round and compare ids. It
takes about ten minutes and needs nothing from the maintainer. See
[Auditing a project](operating/auditing.md).

### Where is the API reference?

[docs.rs/dedalo](https://docs.rs/dedalo), versioned per release. This handbook
is the narrative documentation; the reference is generated from the source.

### I found a wrong amount. Where do I report it?

**Privately**, as a [security advisory][advisory] — never a public issue. A
payout that pays the wrong amount, the wrong address, or twice is a security
issue here, and so is credit assigned to the wrong person. See
[Reporting a vulnerability](trust/security.md).

[squash]: https://github.com/dedalo-org/dedalo/issues/13
[reviews]: https://github.com/dedalo-org/dedalo/issues/12
[chain]: https://github.com/dedalo-org/dedalo/issues/15
[advisory]: https://github.com/dedalo-org/dedalo/security/advisories/new
