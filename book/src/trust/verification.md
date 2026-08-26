# What is proved, and what is only tested

Tests sample. Some of this codebase is proved, and the difference is worth
being exact about — so it is written down per module in
[`verification.toml`][manifest] rather than implied by a badge.

## The methods

| Method | What passing means |
| --- | --- |
| **exhaustive** | Every value in a complete finite domain was tried. **No counterexample exists in that domain.** |
| **property** | Thousands of generated samples. **Not a proof** — a rare counterexample can survive, and one did. |
| **tests** | Hand-picked cases. |
| **proofs** | The module is not verified; it *is* verification. A `proofs.rs` compiles only under `cfg(test)` and ships in no release. |
| **exempt** | The module decides neither how much money moves nor where it goes. Must have zero arithmetic sites, and must not build an address. |
| **binding** | A document the code is checked against. |

Being honest about which is which is the point. A module marked `property` is
not proved, and the manifest is where that is admitted rather than implied.

## What is exhaustively proved today

- **Every fee schedule** — all 50,005,000 `(protocol_bps, treasury_bps)` pairs
  that validate, against the amounts where integer arithmetic breaks: the three
  slices sum to exactly the gross, and no fee is ever rounded up.
- **Every basis-point value** — all 65,536: floor-exact, never exceeding the
  input.
- **Every small weight vector** — all 2,800 of length ≤ 4 with weights ≤ 6:
  shares conserve the total, a zero weight is never paid, a larger weight never
  receives less.
- **Every tree shape to 64 claims** — each claim proves against its own root,
  and against no other claim's proof.

The gross amount is not enumerable, so it is pinned to the values where integer
arithmetic breaks rather than sampled randomly. Longer weight vectors and
larger weights are sampled by property tests, and the manifest says so.

Run them:

```bash
cargo test --release -- --ignored
```

They are `#[ignore]` because fifty million fee schedules is not a thing to put
in the inner loop of `cargo test`. `ws-check` runs them.

## The manifest is a gate, not a table

`tests/verification_manifest.rs` is what keeps the table above from becoming
decoration. It fails the build when:

- a module under `src/` is **not accounted for** in `verification.toml`;
- a declared proof's **test has been deleted**;
- the **money arithmetic in a module changes count** — every module records
  `arithmetic_sites`, and if the number moves, somebody has to look;
- a module claiming **exemption starts doing arithmetic** or starts building an
  address.

So a new module cannot be merged without someone deciding what verifies it, and
an exemption cannot quietly stop being true. Adding a multiplication to the
money path is a build failure until it is acknowledged.

## The layers of the test suite

| Layer | What it holds down |
| --- | --- |
| unit | The arithmetic, parsing and config, next to the code. |
| property (`proptest`) | The money invariants, over thousands of generated rounds. |
| **adversarial** | **What the system must refuse. Every test is a way money could be lost.** |
| end-to-end | The library against real repositories with real merge commits. |
| CLI | Exit codes and the `--json` shape `action.yml` parses. |

[`tests/adversarial.rs`][adv] is the one to read first. It asks whether Dedalo
can be made to compute a *wrong* answer: whether two different plans can share
an id, whether one account spelled two ways can be paid twice, whether a plan
id can steer a filesystem path, whether a mistyped address survives its
checksum.

Each test marked `FOUND:` is a regression test for a defect that was **real
here** — not a hypothetical. Including one where the defect turned out to be
the claim in the README rather than the code.

## Two things deliberately not claimed

**`property` is not a proof.** It is labelled as such everywhere it appears. A
rare counterexample can survive thousands of samples, and one did: the EIP-55
collisions that used to be pinned in `tests/adversarial.rs` were found by
reasoning about the encoding, not by generating inputs. That chain is gone, and
the lesson is not — the address suite now compares against an independent
base58 decoder in both directions rather than against itself.

**There is no `smt` row any more, and that is a loss worth naming.** The
previous vault was Solidity, and `solc --model-checker-engine bmc` discharged
all ten of its arithmetic conditions with a solver — a stronger statement than
any test. Rust has no equivalent that terminates on this codebase; Kani was
measured and rejected. Those conditions are now covered by tests rather than
proved.

What was gained is that the rules are in the same language as everything else,
tested with the same machinery, and readable without a second toolchain. That
is a real trade, and it went in the direction of legibility at the cost of
strength. Anyone deciding whether to trust the vault should weigh it knowing
which way it went.

## The vault

The rules a deployed contract enforces are ordinary Rust in
[`src/chain/vault`][vault], with a test per way it must refuse. `Refusal` is
the specification, and a test asserts that no two refusals share a sentence —
so a revert reason identifies exactly one rule rather than a family of them.

It is **unaudited and undeployed**. Tested by its author, reasoned about by its
author, and never having held a coin. See
[Before real funds move](../operating/multisig.md#before-real-funds-move).

## Reading the manifest yourself

```bash
grep -A3 '^\[modules' verification.toml
```

Each entry names the method, the number of arithmetic sites, the harnesses that
back a claim, and a note saying what the claim covers and what it does not. If
a module's note says something is sampled rather than proved, that is the
honest state of it.

[manifest]: https://github.com/dedalo-org/dedalo/blob/main/verification.toml
[adv]: https://github.com/dedalo-org/dedalo/blob/main/tests/adversarial.rs
[vault]: https://github.com/dedalo-org/dedalo/tree/main/src/chain/vault
