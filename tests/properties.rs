//! Property tests for the arithmetic that decides what people are paid.
//!
//! The unit tests pin down specific cases; these hammer the same invariants
//! with thousands of generated ones. Every property here is a promise the
//! README makes, so a failure is a bug in the promise, not in the test.

use dedalo::attribution::{Attribution, Contribution};
use dedalo::config::Config;
use dedalo::git::Author;
use dedalo::identity::Identity;
use dedalo::money::{Amount, Asset};
use dedalo::payout::{PayeeKind, PlanBuilder, PlanRange};
use dedalo::treasury::FeeSchedule;
use dedalo::wallet::Address;
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

/// Build a config whose wallets and identities cover `contributor_count` people.
fn config_with(contributor_count: usize, fees: FeeSchedule) -> Config {
    let mut config = Config::template("proptest");
    config.asset = Asset::native("TEST", "testnet", 6);
    config.fees = fees;
    config.wallets.treasury = Address::parse("0x2222222222222222222222222222222222222222").unwrap();
    config.wallets.open_collective =
        Address::parse("0x3333333333333333333333333333333333333333").unwrap();
    config.identities = (0..contributor_count)
        .map(|i| {
            Identity::parse(format!("dev{i}"), &format!("0x{:040x}", i + 1))
                .unwrap()
                .with_email(format!("dev{i}@example.com"))
        })
        .collect();
    config
}

fn attribution_with(scores: &[u128]) -> Attribution {
    let contributions: Vec<Contribution> = scores
        .iter()
        .enumerate()
        .map(|(i, score)| Contribution {
            author: Author::new(format!("Dev {i}"), format!("dev{i}@example.com")),
            score: *score,
            merges: 1,
            commits: 1,
            insertions: 1,
            deletions: 0,
        })
        .collect();
    Attribution {
        merges_analysed: 1,
        total_score: contributions.iter().map(|c| c.score).sum(),
        contributions,
    }
}

fn range() -> PlanRange {
    PlanRange {
        branch: "main".into(),
        from_commit: None,
        to_commit: "0".repeat(40),
        merges: 1,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Whatever the scores and fees, a plan pays out exactly what it was given
    /// and passes its own verification.
    #[test]
    fn plans_always_balance_and_verify(
        scores in prop::collection::vec(0u128..=1_000_000, 1..12),
        gross in amount(),
        fees in fees(),
    ) {
        let config = config_with(scores.len(), fees);
        let attribution = attribution_with(&scores);
        let plan = PlanBuilder::new(&config, &attribution, range(), gross)
            .created_at(0)
            .build()
            .expect("a plan must always be constructible");

        plan.verify().expect("a freshly built plan must verify");
        prop_assert_eq!(plan.total().unwrap(), gross);

        // Contributors together receive exactly the contributor pool.
        let paid: u128 = plan.contributors().map(|i| i.amount.base_units()).sum();
        prop_assert_eq!(paid, plan.split.contributors.base_units());

        // The fee recipients appear exactly once each.
        prop_assert_eq!(plan.items.iter().filter(|i| i.kind == PayeeKind::Protocol).count(), 1);
        prop_assert_eq!(plan.items.iter().filter(|i| i.kind == PayeeKind::Treasury).count(), 1);
    }

    /// A plan id depends on the outcome, and on nothing else.
    #[test]
    fn plan_ids_ignore_the_clock(
        scores in prop::collection::vec(1u128..=1_000_000, 1..8),
        gross in amount(),
    ) {
        let config = config_with(scores.len(), FeeSchedule::default());
        let attribution = attribution_with(&scores);

        let early = PlanBuilder::new(&config, &attribution, range(), gross)
            .created_at(0).build().unwrap();
        let late = PlanBuilder::new(&config, &attribution, range(), gross)
            .created_at(1_900_000_000).build().unwrap();

        prop_assert_eq!(&early.id, &late.id);
    }

    /// Changing what anyone receives changes the plan id.
    #[test]
    fn tampering_with_a_plan_breaks_its_id(
        scores in prop::collection::vec(1u128..=1_000_000, 1..8),
        gross in (1u128..=1_000_000_000_000).prop_map(Amount::from_base_units),
        index in 0usize..8,
    ) {
        let config = config_with(scores.len(), FeeSchedule::default());
        let plan = PlanBuilder::new(&config, &attribution_with(&scores), range(), gross)
            .created_at(0).build().unwrap();

        let mut tampered = plan.clone();
        let target = index % tampered.items.len();
        tampered.items[target].amount =
            Amount::from_base_units(tampered.items[target].amount.base_units() + 1);

        prop_assert!(tampered.verify().is_err(), "an edited plan must not verify");
    }

    /// Several identities behind one wallet are paid once, for the same total.
    #[test]
    fn one_wallet_is_paid_once(
        scores in prop::collection::vec(1u128..=1_000_000, 2..10),
        gross in amount(),
    ) {
        let mut config = config_with(scores.len(), FeeSchedule::default());
        // Point every identity at the same address.
        for identity in &mut config.identities {
            identity.wallet = Some(Address::parse("0x000000000000000000000000000000000000c0de").unwrap());
        }
        let plan = PlanBuilder::new(&config, &attribution_with(&scores), range(), gross)
            .created_at(0).build().unwrap();

        prop_assert_eq!(plan.contributors().count(), 1);
        prop_assert_eq!(plan.total().unwrap(), gross);
        plan.verify().unwrap();
    }
}
