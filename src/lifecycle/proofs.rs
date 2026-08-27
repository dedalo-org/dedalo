//! Proofs about the rules a funded project follows.
//!
//! Same two kinds as [`money::proofs`](crate::money::proofs), and the same
//! distinction: **property** tests sample, **exhaustive** tests enumerate and
//! passing one means no counterexample exists in the domain it names.
//!
//! What is proved here is narrower than it looks, and the narrowness is the
//! point. These are statements about arithmetic — that a split conserves, that
//! a ladder cannot go backwards. They say nothing about whether the thresholds
//! or the slices are *wise*, which is a judgement nobody can automate and
//! which the issues in `#66` are for.

use super::roles::{Distribution, Role, Roles};
use super::{Ladder, RevenueSchedule, Stage};
use crate::chain::wallet::Address;
use crate::money::Amount;
use crate::money::treasury::BPS_DENOMINATOR;
use proptest::prelude::*;

/// A distinct, on-curve wallet for a generated role.
fn wallet(tag: u64) -> Address {
    let mut raw = [0u8; 32];
    // Offset by one: tag zero with a zero nudge is thirty-two zero bytes,
    // which is the System Program — a real address, on the curve, and the one
    // address `Roles::validate` refuses because anything sent there is gone.
    raw[..8].copy_from_slice(&(tag + 1).to_le_bytes());
    for nudge in 0..=u8::MAX {
        raw[31] = nudge;
        let candidate = Address::from_pubkey_bytes(raw);
        if candidate.is_on_curve() && !candidate.is_zero() {
            return candidate;
        }
    }
    unreachable!("some nudge of the last byte lands on the curve")
}

/// A schedule that always leaves contributors something.
///
/// Each slice takes what the previous ones left, so the whole valid space is
/// reachable and nothing has to be filtered out afterwards.
fn valid_schedule(a: u32, b: u32, c: u32) -> RevenueSchedule {
    let refinance = a % BPS_DENOMINATOR;
    let staking = b % (BPS_DENOMINATOR - refinance).max(1);
    let roles = c % (BPS_DENOMINATOR - refinance - staking).max(1);
    RevenueSchedule {
        refinance_bps: refinance as u16,
        staking_bps: staking as u16,
        roles_bps: roles as u16,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Every valid revenue schedule conserves every base unit it is given.
    ///
    /// The invariant the module exists for. Three slices are multiplications
    /// that round down and the fourth is a subtraction, which is what makes
    /// this true for *every* input rather than for most of them — a fourth
    /// multiplication would lose a unit whenever the four quotients did not
    /// sum back.
    #[test]
    fn every_valid_schedule_conserves_the_revenue(
        a in 0u32..10_000,
        b in 0u32..10_000,
        c in 0u32..10_000,
        harvested in 0u128..=1_000_000_000_000,
    ) {
        // Drawn from what the previous slices left, rather than three
        // independent values filtered afterwards: three independent
        // basis-point values almost never sum below the denominator, so a
        // `prop_assume` here rejected a thousand cases for every one it kept
        // and the test aborted rather than passing vacuously.
        let schedule = valid_schedule(a, b, c);
        prop_assert!(schedule.validate().is_ok());

        let split = schedule.split(Amount::from_base_units(harvested)).unwrap();
        prop_assert!(split.balances());
        prop_assert_eq!(split.harvested.base_units(), harvested);
    }

    /// No named slice is ever rounded up.
    ///
    /// Stated separately from conservation because the two are different
    /// promises and only one of them says who pays for the rounding. A split
    /// could conserve perfectly while taking a unit from contributors.
    #[test]
    fn no_slice_ever_rounds_up_against_contributors(
        a in 0u32..10_000,
        b in 0u32..10_000,
        c in 0u32..10_000,
        harvested in 0u128..=1_000_000_000,
    ) {
        let schedule = valid_schedule(a, b, c);
        let (refinance, staking, roles) =
            (schedule.refinance_bps, schedule.staking_bps, schedule.roles_bps);
        let split = schedule.split(Amount::from_base_units(harvested)).unwrap();

        let floor = |bps: u16| harvested * u128::from(bps) / u128::from(BPS_DENOMINATOR);
        prop_assert_eq!(split.refinance.base_units(), floor(refinance));
        prop_assert_eq!(split.staking.base_units(), floor(staking));
        prop_assert_eq!(split.roles.base_units(), floor(roles));

        // And contributors get at least their nominal share, never less.
        prop_assert!(split.contributors.base_units() >= floor(schedule.contributor_bps()));
    }

    /// A schedule that leaves contributors nothing is always refused.
    #[test]
    fn a_schedule_that_pays_contributors_nothing_never_splits(
        refinance in 0u16..=10_000,
        staking in 0u16..=10_000,
        roles in 0u16..=10_000,
    ) {
        let schedule = RevenueSchedule {
            refinance_bps: refinance,
            staking_bps: staking,
            roles_bps: roles,
        };
        let consumes_everything =
            u32::from(refinance) + u32::from(staking) + u32::from(roles) >= BPS_DENOMINATOR;
        prop_assert_eq!(schedule.validate().is_err(), consumes_everything);
        prop_assert_eq!(
            schedule.split(Amount::from_base_units(10_000)).is_err(),
            consumes_everything
        );
    }

    /// The ladder only ever goes up.
    ///
    /// A project that has raised more is never at a lower stage. This is what
    /// makes tokenisation irreversible by arithmetic rather than by promise —
    /// a token that could be withdrawn by the project spending its reserve is
    /// not a token anybody would hold.
    #[test]
    fn raising_more_never_lowers_the_stage(
        stake in prop::option::of(0u128..=1_000_000),
        token in prop::option::of(0u128..=1_000_000),
        a in 0u128..=1_000_000,
        b in 0u128..=1_000_000,
    ) {
        let ladder = Ladder {
            stake_threshold: stake.map(Amount::from_base_units),
            token_threshold: token.map(Amount::from_base_units),
        };
        let (low, high) = if a <= b { (a, b) } else { (b, a) };
        let lower = ladder.stage_for(Amount::from_base_units(low));
        let higher = ladder.stage_for(Amount::from_base_units(high));
        prop_assert!(higher >= lower, "{high} gave {higher:?}, {low} gave {lower:?}");
    }

    /// A role distribution conserves the pool, whatever the weights.
    ///
    /// Including the case nobody plans for: every weight zero, where the whole
    /// pool must be declared undistributed rather than silently absorbed.
    #[test]
    fn every_role_distribution_conserves_the_pool(
        weights in prop::collection::vec(0u64..=1_000, 1..8),
        pool in 0u128..=1_000_000_000,
    ) {
        let roles = Roles(
            weights
                .iter()
                .enumerate()
                .map(|(index, weight)| Role {
                    name: format!("role{index}"),
                    weight: *weight,
                    wallets: vec![wallet(index as u64)],
                })
                .collect(),
        );

        let distribution =
            Distribution::for_period(&roles, Amount::from_base_units(pool)).unwrap();
        prop_assert!(distribution.balances());

        let paid: u128 = distribution.shares.iter().map(|s| s.amount.base_units()).sum();
        prop_assert_eq!(paid + distribution.undistributed.base_units(), pool);

        // A wallet with no weight is never paid, which is the role equivalent
        // of "a zero weight is never paid" in the payout split.
        for share in &distribution.shares {
            if share.weight == 0 {
                prop_assert_eq!(share.amount, Amount::ZERO);
            }
        }
    }

    /// One wallet is paid once, however many roles it holds.
    ///
    /// The mirror of `payout`'s invariant 5, on a different set. Two entries
    /// paying one account is a double payment and a table that lies about how
    /// many people there are.
    #[test]
    fn a_wallet_in_many_roles_is_paid_once(
        weights in prop::collection::vec(1u64..=100, 1..6),
        pool in 1u128..=1_000_000,
    ) {
        // Every role names the same wallet.
        let only = wallet(42);
        let roles = Roles(
            weights
                .iter()
                .enumerate()
                .map(|(index, weight)| Role {
                    name: format!("role{index}"),
                    weight: *weight,
                    wallets: vec![only.clone()],
                })
                .collect(),
        );

        let distribution =
            Distribution::for_period(&roles, Amount::from_base_units(pool)).unwrap();
        prop_assert_eq!(distribution.shares.len(), 1);
        prop_assert_eq!(
            distribution.shares[0].weight,
            weights.iter().map(|w| u128::from(*w)).sum::<u128>()
        );
        prop_assert_eq!(distribution.shares[0].amount.base_units(), pool);
    }
}

/// Exhaustive over every basis-point value a single slice can take.
///
/// The domain is the 10,000 values `refinance_bps` may hold with the other two
/// slices fixed at zero, against the amounts where integer division is most
/// likely to lose a unit. It is a proof about that domain and **not** about the
/// three-dimensional space of whole schedules, which is 10^12 and is sampled by
/// the property above instead.
#[test]
#[ignore = "exhaustive: run with --ignored"]
fn every_basis_point_value_of_one_slice_conserves_the_revenue() {
    // The amounts where flooring bites: one below, one at, and one above each
    // power that divides the denominator.
    const AMOUNTS: [u128; 9] = [0, 1, 2, 9_999, 10_000, 10_001, 99_999, 100_000, 100_001];

    let mut checked = 0u32;
    for bps in 0..BPS_DENOMINATOR as u16 {
        let schedule = RevenueSchedule {
            refinance_bps: bps,
            staking_bps: 0,
            roles_bps: 0,
        };
        if schedule.validate().is_err() {
            continue;
        }
        for amount in AMOUNTS {
            let split = schedule.split(Amount::from_base_units(amount)).unwrap();
            assert!(split.balances(), "{bps} bps of {amount} did not balance");
            assert_eq!(
                split.refinance.base_units(),
                amount * u128::from(bps) / u128::from(BPS_DENOMINATOR),
                "{bps} bps of {amount} did not round down"
            );
        }
        checked += 1;
    }

    // The domain asserts its own size: a proof that quietly stops covering
    // what it claims is worse than no proof.
    assert_eq!(
        checked,
        u32::from(BPS_DENOMINATOR as u16 - 1) + 1,
        "expected every basis-point value below the denominator to be valid alone"
    );
}

/// Exhaustive over every stage transition a two-rung ladder can express.
///
/// Every ordering of (stake, token, raised) over a small domain, which is
/// enough to cover each of the nine shapes the two `Option`s and the
/// comparison can take.
#[test]
#[ignore = "exhaustive: run with --ignored"]
fn every_ladder_shape_reports_a_stage_that_matches_its_thresholds() {
    const VALUES: [Option<u128>; 4] = [None, Some(0), Some(50), Some(100)];

    for stake in VALUES {
        for token in VALUES {
            let ladder = Ladder {
                stake_threshold: stake.map(Amount::from_base_units),
                token_threshold: token.map(Amount::from_base_units),
            };
            for raised in 0..=120u128 {
                let raised = Amount::from_base_units(raised);
                let stage = ladder.stage_for(raised);

                // The stage is exactly what the thresholds say it is, derived
                // here independently of the implementation.
                let tokenised = token.is_some_and(|t| raised.base_units() >= t);
                let staked = stake.is_some_and(|s| raised.base_units() >= s);
                let expected = if tokenised {
                    Stage::Tokenised
                } else if staked {
                    Stage::Staked
                } else {
                    Stage::Raising
                };
                assert_eq!(
                    stage, expected,
                    "stake={stake:?} token={token:?} raised={raised:?}"
                );

                // And whatever remains to the next rung, adding it reaches one.
                if let Some(remaining) = ladder.remaining_to_next(raised) {
                    let reached = raised.checked_add(remaining).unwrap();
                    assert!(
                        ladder.stage_for(reached) > stage,
                        "adding the remaining amount must reach the next stage"
                    );
                }
            }
        }
    }
}
