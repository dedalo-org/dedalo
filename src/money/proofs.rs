//! Proofs about the arithmetic that decides what people are paid.
//!
//! Two kinds, and the difference matters:
//!
//! - **property** tests sample. Thousands of generated inputs, which is strong
//!   evidence and not a proof.
//! - **exhaustive** tests enumerate. Every value in a complete finite domain is
//!   tried, so passing means no counterexample exists in that domain.
//!
//! Each exhaustive test names the domain it covers *and* the domain it does
//! not, and asserts its own size — a proof that quietly stops covering what it
//! claims is worse than no proof.
//!
//! The exhaustive ones are slow by construction and marked `#[ignore]`, so
//! `cargo test` stays fast. `ws-check` and the `verification` CI job run them
//! with `--ignored`.

use super::Amount;
use super::treasury::{BPS_DENOMINATOR, FeeSchedule};
use proptest::prelude::*;

/// Realistic money: up to a quintillion base units, which is ~10^12 USDC.
fn amount() -> impl Strategy<Value = Amount> {
    (0u128..=1_000_000_000_000_000_000).prop_map(Amount::from_base_units)
}

/// Attribution weights stay well inside the range where `amount * weight`
/// cannot overflow a u128; the overflow path is tested separately.
fn weights() -> impl Strategy<Value = Vec<u128>> {
    prop::collection::vec(0u128..=1_000_000_000, 0..40)
}

/// Any fee schedule that leaves something for contributors.
fn fees() -> impl Strategy<Value = FeeSchedule> {
    (0u16..9_000, 0u16..9_000)
        .prop_filter("fees must leave a contributor share", |(p, t)| {
            (*p as u32) + (*t as u32) < 10_000
        })
        .prop_map(|(protocol_bps, treasury_bps)| FeeSchedule {
            protocol_bps,
            treasury_bps,
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// The headline promise: a split creates and destroys nothing.
    #[test]
    fn split_conserves_the_total(total in amount(), weights in weights()) {
        let shares = total.split_by_weights(&weights).expect("must not overflow");
        prop_assert_eq!(shares.len(), weights.len());

        let sum: u128 = shares.iter().map(|s| s.base_units()).sum();
        let total_weight: u128 = weights.iter().sum();
        if total_weight == 0 {
            // Nobody earned anything, so nothing is handed out.
            prop_assert_eq!(sum, 0);
        } else {
            prop_assert_eq!(sum, total.base_units());
        }
    }

    /// Nobody can be paid more than the whole round.
    #[test]
    fn no_share_exceeds_the_total(total in amount(), weights in weights()) {
        for share in total.split_by_weights(&weights).unwrap() {
            prop_assert!(share <= total);
        }
    }

    /// A larger weight is never paid less than a smaller one.
    #[test]
    fn shares_are_monotone_in_weight(total in amount(), weights in weights()) {
        let shares = total.split_by_weights(&weights).unwrap();
        for i in 0..weights.len() {
            for j in 0..weights.len() {
                if weights[i] > weights[j] {
                    prop_assert!(
                        shares[i] >= shares[j],
                        "weight {} got {} but weight {} got {}",
                        weights[i], shares[i], weights[j], shares[j]
                    );
                }
            }
        }
    }

    /// A zero weight is never paid, however large the round.
    #[test]
    fn zero_weight_is_never_paid(total in amount(), mut weights in weights()) {
        weights.push(0);
        let shares = total.split_by_weights(&weights).unwrap();
        prop_assert!(shares.last().unwrap().is_zero());
    }

    /// Splitting is stable: the same inputs always produce the same shares.
    #[test]
    fn split_is_deterministic(total in amount(), weights in weights()) {
        prop_assert_eq!(
            total.split_by_weights(&weights).unwrap(),
            total.split_by_weights(&weights).unwrap()
        );
    }

    /// Cutting a round always adds back up to the round.
    #[test]
    fn fee_split_is_balanced(gross in amount(), fees in fees()) {
        let split = fees.split(gross).expect("a valid schedule must split");
        prop_assert!(split.is_balanced());
        prop_assert_eq!(
            split.protocol.base_units() + split.treasury.base_units()
                + split.contributors.base_units(),
            gross.base_units()
        );
    }

    /// Fees round down, so rounding dust can only ever help contributors.
    #[test]
    fn fees_never_round_up(gross in amount(), fees in fees()) {
        let split = fees.split(gross).unwrap();
        let nominal_protocol = gross.base_units() * fees.protocol_bps as u128 / 10_000;
        let nominal_treasury = gross.base_units() * fees.treasury_bps as u128 / 10_000;
        prop_assert_eq!(split.protocol.base_units(), nominal_protocol);
        prop_assert_eq!(split.treasury.base_units(), nominal_treasury);
        prop_assert!(
            split.contributors.base_units()
                >= gross.base_units() * fees.contributor_bps() as u128 / 10_000
        );
    }

    /// Decimal rendering is lossless: what is parsed comes back out.
    #[test]
    fn decimal_strings_round_trip(units in 0u128..=u128::from(u64::MAX), decimals in 0u8..=18) {
        let value = Amount::from_base_units(units);
        let rendered = value.to_decimal_string(decimals);
        prop_assert_eq!(Amount::parse(&rendered, decimals).unwrap(), value);
    }

    /// A rendered amount never gains precision the asset does not have.
    #[test]
    fn rendering_respects_asset_precision(units in 0u128..=u128::from(u64::MAX), decimals in 1u8..=18) {
        let rendered = Amount::from_base_units(units).to_decimal_string(decimals);
        if let Some((_, fraction)) = rendered.split_once('.') {
            prop_assert!(fraction.len() <= decimals as usize);
            prop_assert!(!fraction.ends_with('0'), "trailing zeros in {rendered}");
        }
    }

    /// Overflow is reported, never wrapped into a smaller number.
    #[test]
    fn huge_values_error_instead_of_wrapping(weight in (u128::MAX / 2)..u128::MAX) {
        let result = Amount::from_base_units(u128::MAX).split_by_weights(&[weight, weight]);
        prop_assert!(result.is_err(), "overflow must not silently wrap");
    }
}

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
