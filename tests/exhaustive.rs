//! Proofs by exhaustion.
//!
//! A property test samples. These enumerate: every value the domain contains
//! is tried, so passing is not evidence that a counterexample is unlikely — it
//! is a proof that none exists, for that domain.
//!
//! That is worth stating precisely, because it is easy to overclaim. Each test
//! below names the domain it covers and the domain it does not. Where a
//! parameter is unbounded — a `u128` amount — the enumeration is over the
//! bounded parameters, with the unbounded one pinned to values chosen because
//! they are where arithmetic breaks: zero, one, a prime, either side of the
//! rounding boundary, and the largest value that cannot overflow.
//!
//! These are slow by construction and marked `#[ignore]`, so `cargo test` stays
//! fast. `ws-check` and the `verification` CI job run them with `--ignored`.
//! `verification.toml` records which module each one covers, and
//! `tests/verification_manifest.rs` fails if a module claims a proof that does
//! not exist.

use dedalo::merkle::{Claim, ClaimTree};
use dedalo::money::Amount;
use dedalo::treasury::{BPS_DENOMINATOR, FeeSchedule};
use dedalo::wallet::Address;

/// Amounts chosen because they are where integer arithmetic goes wrong, not
/// because they are typical.
fn adversarial_amounts() -> Vec<u128> {
    vec![
        0,
        1,
        3,
        7,
        9_999,
        10_000,
        10_001,
        999_999,
        1_000_000,
        u32::MAX as u128,
        u64::MAX as u128,
        // The largest amount that survives `checked_mul(10_000)`, so the
        // boundary between "computes" and "errors" is covered from below.
        u128::MAX / 10_000,
    ]
}

/// **Domain: every valid fee schedule.** All 50,005,000 pairs of
/// `(protocol_bps, treasury_bps)` that `validate` accepts, against the
/// adversarial amounts.
///
/// Proves, for that whole domain: the three slices sum to exactly the gross,
/// and neither fee is ever rounded up.
#[test]
#[ignore = "exhaustive: ~30s"]
fn every_valid_fee_schedule_conserves_the_round() {
    let amounts = adversarial_amounts();
    let mut checked: u64 = 0;

    for protocol in 0..=BPS_DENOMINATOR as u16 {
        for treasury in 0..=(BPS_DENOMINATOR as u16 - protocol) {
            let schedule = FeeSchedule {
                protocol_bps: protocol,
                treasury_bps: treasury,
            };
            if schedule.validate().is_err() {
                continue;
            }
            checked += 1;

            for &gross in &amounts {
                let Ok(split) = schedule.split(Amount::from_base_units(gross)) else {
                    continue;
                };
                assert!(
                    split.is_balanced(),
                    "{protocol}/{treasury} bps of {gross} does not balance"
                );

                // Fees round down: taking the fee and scaling back up can
                // never exceed what was there to begin with.
                let protocol_units = split.protocol.base_units();
                let treasury_units = split.treasury.base_units();
                assert!(
                    protocol_units * 10_000 <= gross * protocol as u128,
                    "protocol fee rounded up at {protocol} bps of {gross}"
                );
                assert!(
                    treasury_units * 10_000 <= gross * treasury as u128,
                    "treasury fee rounded up at {treasury} bps of {gross}"
                );
            }
        }
    }

    assert_eq!(
        checked, 50_005_000,
        "the valid fee-schedule domain changed size; the proof no longer covers what it claims"
    );
}

/// **Domain: every basis-point value.** All 65,536 values a `u16` can hold —
/// including the ones `validate` would reject, because `Amount::bps` is a
/// public function that does not know about fee schedules.
///
/// Proves: the result never exceeds the input, and is always the floor of the
/// exact fraction.
#[test]
#[ignore = "exhaustive: ~5s"]
fn every_basis_point_value_rounds_down_and_never_exceeds_the_input() {
    for &gross in &adversarial_amounts() {
        let amount = Amount::from_base_units(gross);
        for bps in 0..=u16::MAX {
            let Ok(taken) = amount.bps(bps) else {
                // Only the overflow path may refuse, and only above the
                // boundary the adversarial set brackets.
                assert!(
                    gross.checked_mul(bps as u128).is_none(),
                    "bps({bps}) of {gross} refused without overflowing"
                );
                continue;
            };
            let exact = gross * bps as u128;
            assert_eq!(
                taken.base_units(),
                exact / 10_000,
                "bps({bps}) of {gross} is not the floor"
            );
            if bps <= 10_000 {
                assert!(
                    taken.base_units() <= gross,
                    "bps({bps}) of {gross} exceeded the input"
                );
            }
        }
    }
}

/// **Domain: every weight vector of up to four entries with weights 0..=6.**
/// 2,800 vectors, against the adversarial amounts.
///
/// Proves, for that domain: the shares sum to exactly the total, a zero weight
/// is never paid, and a larger weight never receives less than a smaller one.
/// It does **not** cover longer vectors or larger weights — the property suite
/// samples those.
#[test]
#[ignore = "exhaustive: ~10s"]
fn every_small_weight_vector_conserves_the_total() {
    /// Every vector of length 1..=4 with each weight in 0..=6, as a counter in
    /// base seven. Written as an odometer the first time, which returned from
    /// the whole function on the first carry-out and therefore enumerated
    /// seven vectors while claiming two thousand eight hundred. `clippy::
    /// never_loop` caught it; the assertion below is what stops it recurring.
    fn vectors() -> Vec<Vec<u128>> {
        const MAX: usize = 6;
        const BASE: usize = MAX + 1;

        let mut out = Vec::new();
        for len in 1..=4usize {
            for mut n in 0..BASE.pow(len as u32) {
                let mut vector = Vec::with_capacity(len);
                for _ in 0..len {
                    vector.push((n % BASE) as u128);
                    n /= BASE;
                }
                out.push(vector);
            }
        }
        out
    }

    let all = vectors();
    assert_eq!(
        all.len(),
        7 + 49 + 343 + 2_401,
        "the weight-vector domain changed size; the proof no longer covers what it claims"
    );

    let amounts = adversarial_amounts();
    for weights in all {
        for &total in &amounts {
            let shares = Amount::from_base_units(total)
                .split_by_weights(&weights)
                .expect("small weights never overflow");
            let sum: u128 = shares.iter().map(|s| s.base_units()).sum();
            let weight_sum: u128 = weights.iter().sum();

            if weight_sum == 0 {
                assert_eq!(sum, 0, "{weights:?} of {total} paid out of nothing");
                continue;
            }
            assert_eq!(sum, total, "{weights:?} of {total} did not conserve");

            for (index, weight) in weights.iter().enumerate() {
                if *weight == 0 {
                    assert_eq!(
                        shares[index].base_units(),
                        0,
                        "{weights:?} of {total} paid a zero weight"
                    );
                }
                for (other, other_weight) in weights.iter().enumerate() {
                    if weight > other_weight {
                        assert!(
                            shares[index] >= shares[other],
                            "{weights:?} of {total}: weight {weight} received less than {other_weight}"
                        );
                    }
                }
            }
        }
    }
}

/// **Domain: every tree size from one to sixty-four claims.**
///
/// Proves, for that domain: every claim in a tree verifies against its root,
/// and no claim verifies against another claim's proof. Sixty-four covers
/// every shape the promotion rule produces — six full levels, and every odd
/// count that leaves a node without a sibling.
#[test]
#[ignore = "exhaustive: ~20s"]
fn every_tree_size_up_to_sixty_four_proves_exactly_its_own_claims() {
    fn address(seed: u64) -> Address {
        let body: String = (0..20)
            .map(|i| format!("{:02x}", (seed + i) as u8))
            .collect();
        Address::parse(&format!("0x{body}")).unwrap()
    }

    for size in 1..=64u64 {
        let claims: Vec<Claim> = (0..size)
            .map(|index| Claim {
                index,
                account: address(index),
                amount: Amount::from_base_units(u128::from(index) + 1),
            })
            .collect();
        let tree = ClaimTree::new(claims).unwrap();
        let root = tree.root();

        for (index, claim) in tree.claims().iter().enumerate() {
            let proof = tree.proof(index).unwrap();
            let leaf = claim.leaf().unwrap();
            assert!(
                ClaimTree::verify(root, leaf, &proof),
                "size {size}: claim {index} did not verify against its own root"
            );

            // No other claim in the tree may ride this proof.
            for (other, other_claim) in tree.claims().iter().enumerate() {
                if other == index {
                    continue;
                }
                assert!(
                    !ClaimTree::verify(root, other_claim.leaf().unwrap(), &proof),
                    "size {size}: claim {other} verified with claim {index}'s proof"
                );
            }
        }
    }
}
