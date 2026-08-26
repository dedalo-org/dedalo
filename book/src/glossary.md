# Glossary

**Amount** — an integer count of base units of an **asset**. Never a
float. → [Money](concepts/money.md)

**Asset** — the token contributors are paid in: symbol, decimals, chain, and an
optional contract address. Omitting the contract means the chain's native coin.

**Attribution** — turning merge history into integer contribution weights. Does
not know money exists. → [Attribution](concepts/attribution.md)

**Base unit** — the smallest indivisible quantity of an asset: wei, satoshi,
USDC micro-units. `decimals` says how many make one display unit.

**Basis point (bps)** — one hundredth of a percent. 10,000 bps = 100%. Used
instead of percentages so a share is always an integer.

**Claim** — a contributor taking their share out of a deposited round, proving
membership against the round's Merkle root and paying their own gas.

**Claim window** — 180 days, fixed. After it closes, the depositor may sweep
what was never claimed. Fixed rather than configurable because a depositor who
could choose it could choose a window nobody could claim within.

**Content-addressed** — identified by a hash of its own contents, so the name
changes if the thing changes. Plans and ledger entries both are.

**Co-authored-by** — a git commit trailer naming additional authors. Splits a
commit's score when `split_with_co_authors` is on.

**Dry-run** — the default settlement backend. Computes and verifies everything
a real settlement would, and moves nothing.

**base58** — how a Solana address is written: thirty-two bytes in an alphabet
that leaves out `0`, `O`, `I` and `l` because they are misread. It carries **no
checksum**, so every thirty-two byte value is a valid address.
→ [There is no checksum](concepts/identities.md#there-is-no-checksum)

**Engine** — the type tying a repository, its config and its ledger together.
The shortest path through all four pipeline stages.

**Exhaustive** — a verification method: every value in a complete finite domain
was tried, so no counterexample exists in that domain. Stronger than *property*.
→ [Verification](trust/verification.md)

**Fee schedule** — `protocol_bps` and `treasury_bps`. Taken off the top of every
round, in that order, rounding down.

**Gross** — the full size of a round, before any cut.

**Handle** — the label a contributor appears under. Usually a GitHub username;
nothing checks that, and it is not part of the plan id.

**Identity** — the mapping from one or more git emails to one wallet, under one
handle. The only part of a round a human types in.

**Largest-remainder method** — how an amount is split across weights: floor
each share, then hand out the leftover base units one at a time to the largest
fractional remainders. Guarantees the shares sum to exactly the input.

**Ledger** — the hash-chained record in `.dedalo/`. Each entry names its parent
and hashes over it. → [The ledger](concepts/ledger.md)

**Merge event** — one merge commit on the tracked branch, with the commits it
introduced, their authors and trailers, and its diff against its first parent.
The unit Dedalo pays for.

**Merkle root** — the root of the tree of `(address, amount)` claims for a
round. Deposited on chain; contributors prove against it.

**Milli-point** — the unit of attribution scores. 1 point = 1,000 milli-points,
stored as `u128`, so scoring is integer arithmetic on every machine.

**Payout plan** — the auditable artifact between git history and a transaction.
Pure data, content-addressed. → [Payout plans](concepts/plans.md)

**Plan id** — a plan's content hash, prefixed `ded1`. Excludes `created_at`,
`handle`, `score` and `unresolved` — see [what is in it](concepts/plans.md#the-id).

**Property test** — generated inputs, thousands of samples. **Not a proof**: a
rare counterexample can survive one, and one did.

**Protocol fee** — the share of every round routed to the network's own Open
Collective. What makes the network self-funding rather than grant-dependent.

**Pull model** — a round is deposited once against a Merkle root and each
contributor claims. The alternative, push, sends N transfers and has a
half-finished state. → [Funding from a multisig](operating/multisig.md)

**Refusal** — one of the ways the vault says no. Each has a distinct sentence,
and a test asserts no two share one. The enum is the vault's specification.

**Round** — one funding cycle: a range of merges, an amount, a plan, and a
settlement.

**Settlement** — the fourth stage, and the only one with side effects.
→ [Settlement](concepts/settlement.md)

**On-curve** — whether an address is a point on ed25519, and therefore a
keypair's public key that somebody can sign for. A program-derived address is
deliberately not, and so is every associated token account. A contributor's
wallet must be on-curve; a treasury or a multisig vault legitimately is not.
→ [A wallet must be something somebody can sign for](concepts/identities.md#a-wallet-must-be-something-somebody-can-sign-for)

**PDA** — a program-derived address: an address off the ed25519 curve, which no
keypair can produce a signature for, so only its program can act on it.

**Treasury** — the project's own reserve, funded by `treasury_bps` of every
round.

**Undistributed** — the part of the contributor pool that reached nobody,
because whoever earned it has no wallet on file. Stated in the plan, never
absorbed. Under the pull model it means "not claimed yet", not "lost".

**Unresolved** — the list of contributors who earned a share and could not be
paid, with a reason: `no-wallet`, `excluded` or `ignored`.

**Vault** — the rules a deployed contract enforces, written as pure Rust in
`src/chain/vault`. The deployable in `src/chain/contract` is a thin binding
around it.

**Verification manifest** — `verification.toml`, which records how every module
is verified, and the gate that fails the build when a module is unaccounted
for. → [Verification](trust/verification.md)
