//! Property tests for the plan: the artifact a round is reviewed as.
//!
//! A plan's shape depends on a config, an attribution and a range together,
//! and that product is not a finite domain worth enumerating. Sampled instead,
//! which is evidence and not a proof — and said so here rather than implied.

use crate::attribution::identity::Identity;
use crate::attribution::{Attribution, Contribution};
use crate::chain::wallet::Address;
use crate::config::Config;
use crate::git::Author;
use crate::money::treasury::FeeSchedule;
use crate::money::{Amount, Asset};
use crate::payout::{PayeeKind, PlanBuilder, PlanRange};
use proptest::prelude::*;

/// Realistic money: up to a quintillion base units, which is ~10^12 USDC.
fn amount() -> impl Strategy<Value = Amount> {
    (0u128..=1_000_000_000_000_000_000).prop_map(Amount::from_base_units)
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

/// Build a config whose wallets and identities cover `contributor_count` people.
/// A distinct, on-curve address for a generated contributor.
///
/// On-curve matters: an off-curve address is one nobody can sign for, so a
/// share sent there could never be claimed. `Config::validate` refuses those,
/// and a property suite that generated them would be testing a config the
/// tool rejects.
fn wallet_for(index: usize) -> String {
    let mut raw = [0u8; 32];
    raw[..8].copy_from_slice(&(index as u64).to_le_bytes());
    // Roughly half of all thirty-two byte values are not curve points, so the
    // last byte is stepped until one is. It always terminates well before the
    // range runs out.
    for nudge in 0..=u8::MAX {
        raw[31] = nudge;
        let candidate = crate::chain::wallet::Address::from_pubkey_bytes(raw);
        if candidate.is_on_curve() {
            return candidate.to_string();
        }
    }
    unreachable!("some nudge of the last byte lands on the curve")
}

fn config_with(contributor_count: usize, fees: FeeSchedule) -> Config {
    let mut config = Config::template("proptest");
    config.asset = Asset::native("TEST", "testnet", 6);
    config.fees = fees;
    config.wallets.treasury =
        Address::parse("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
    config.wallets.open_collective =
        Address::parse("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL").unwrap();
    config.identities = (0..contributor_count)
        .map(|i| {
            Identity::parse(format!("dev{i}"), &wallet_for(i))
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
            identity.wallet = Some(Address::parse("So11111111111111111111111111111111111111112").unwrap());
        }
        let plan = PlanBuilder::new(&config, &attribution_with(&scores), range(), gross)
            .created_at(0).build().unwrap();

        prop_assert_eq!(plan.contributors().count(), 1);
        prop_assert_eq!(plan.total().unwrap(), gross);
        plan.verify().unwrap();
    }
}
