//! Adversarial tests: what the system must refuse.
//!
//! The rest of the suite asks whether Dedalo computes the right answer. This
//! one asks whether it can be made to compute a wrong one — by a hostile
//! input, a mistyped address, a race, or a plan edited after review. Every
//! test here corresponds to a way money could actually be lost.
//!
//! Each one that carries a `FOUND:` note is a regression test for a defect
//! that was real in this codebase, not a hypothetical.

use dedalo_core::attribution::{Attribution, AttributionPolicy, Contribution};
use dedalo_core::config::Config;
use dedalo_core::git::{Author, DiffStat, MergeEvent, MergedCommit};
use dedalo_core::identity::Identity;
use dedalo_core::ledger::Ledger;
use dedalo_core::money::{Amount, Asset};
use dedalo_core::payout::{PayeeKind, PayoutPlan, PlanBuilder, PlanRange};
use dedalo_core::settlement::DryRunSettlement;
use dedalo_core::testing::TempRepo;
use dedalo_core::wallet::{Address, ZERO_ADDRESS};
use dedalo_core::{Engine, SettlementOptions};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------------

fn address(nibble: &str) -> Address {
    Address::parse(&format!("0x{}", nibble.repeat(40 / nibble.len()))).unwrap()
}

fn config_with(contributors: usize) -> Config {
    let mut config = Config::template("adversarial");
    config.asset = Asset::native("TEST", "testnet", 6);
    config.wallets.source = address("1");
    config.wallets.treasury = address("2");
    config.wallets.open_collective = address("3");
    config.identities = (0..contributors)
        .map(|i| {
            Identity::parse(format!("dev{i}"), &format!("0x{:040x}", i + 0xa0))
                .unwrap()
                .with_email(format!("dev{i}@example.com"))
        })
        .collect();
    config
}

fn attribution_of(scores: &[u128]) -> Attribution {
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
        to_commit: "d".repeat(40),
        merges: 1,
    }
}

fn plan_of(config: &Config, scores: &[u128], gross: u128) -> PayoutPlan {
    PlanBuilder::new(
        config,
        &attribution_of(scores),
        range(),
        Amount::from_base_units(gross),
    )
    .created_at(0)
    .build()
    .expect("a plan must always be constructible")
}

// ---------------------------------------------------------------------------
// plan identity: the tamper-evidence and the on-chain replay guard
// ---------------------------------------------------------------------------

/// FOUND: fields were concatenated without separators, so an asset `US` on
/// chain `DCbase` hashed identically to `USDC` on `base`. A collision means a
/// forged plan verifies, or a legitimate round is rejected as already paid.
#[test]
fn adjacent_fields_cannot_be_shifted_into_one_another() {
    let cases: Vec<(Config, PlanRange)> = vec![
        // symbol / chain
        {
            let mut c = config_with(0);
            c.asset = Asset::native("US", "DCbase", 6);
            (c, range())
        },
        {
            let mut c = config_with(0);
            c.asset = Asset::native("USDC", "base", 6);
            (c, range())
        },
        // branch / to_commit
        (
            config_with(0),
            PlanRange {
                branch: "mai".into(),
                from_commit: None,
                to_commit: format!("n{}", "d".repeat(40)),
                merges: 1,
            },
        ),
        (
            config_with(0),
            PlanRange {
                branch: "main".into(),
                from_commit: None,
                to_commit: "d".repeat(40),
                merges: 1,
            },
        ),
        // project / symbol
        {
            let mut c = config_with(0);
            c.project.name = "adversaria".into();
            c.asset = Asset::native("lTEST", "testnet", 6);
            (c, range())
        },
    ];

    let mut ids = Vec::new();
    for (config, r) in cases {
        let plan = PlanBuilder::new(
            &config,
            &attribution_of(&[]),
            r,
            Amount::from_base_units(1_000_000),
        )
        .created_at(0)
        .build()
        .unwrap();
        ids.push(plan.id);
    }

    let unique: std::collections::BTreeSet<_> = ids.iter().collect();
    assert_eq!(
        unique.len(),
        ids.len(),
        "two different plans share an id: {ids:#?}"
    );
}

/// Every field the hash claims to cover must actually change it. A field left
/// out of the encoding is a field an attacker can edit for free.
#[test]
fn changing_any_covered_field_changes_the_id() {
    let config = config_with(2);
    let baseline = plan_of(&config, &[3, 1], 1_000_000);

    let mut mutations: Vec<(&str, PayoutPlan)> = Vec::new();

    let mut m = baseline.clone();
    m.project.push('x');
    mutations.push(("project", m));

    let mut m = baseline.clone();
    m.asset.symbol.push('x');
    mutations.push(("asset.symbol", m));

    let mut m = baseline.clone();
    m.asset.chain.push('x');
    mutations.push(("asset.chain", m));

    let mut m = baseline.clone();
    m.asset.decimals += 1;
    mutations.push(("asset.decimals", m));

    let mut m = baseline.clone();
    m.asset.contract = Some(address("7").to_string());
    mutations.push(("asset.contract", m));

    let mut m = baseline.clone();
    m.range.branch.push('x');
    mutations.push(("range.branch", m));

    let mut m = baseline.clone();
    m.range.from_commit = Some("f".repeat(40));
    mutations.push(("range.from_commit", m));

    let mut m = baseline.clone();
    m.range.to_commit = "e".repeat(40);
    mutations.push(("range.to_commit", m));

    let mut m = baseline.clone();
    m.split.gross = Amount::from_base_units(999);
    mutations.push(("split.gross", m));

    let mut m = baseline.clone();
    m.split.protocol = Amount::from_base_units(999);
    mutations.push(("split.protocol", m));

    let mut m = baseline.clone();
    m.split.treasury = Amount::from_base_units(999);
    mutations.push(("split.treasury", m));

    let mut m = baseline.clone();
    m.split.contributors = Amount::from_base_units(999);
    mutations.push(("split.contributors", m));

    let mut m = baseline.clone();
    m.undistributed = Amount::from_base_units(7);
    mutations.push(("undistributed", m));

    let mut m = baseline.clone();
    m.items[0].amount = Amount::from_base_units(1);
    mutations.push(("items[0].amount", m));

    let mut m = baseline.clone();
    m.items[0].wallet = address("9");
    mutations.push(("items[0].wallet", m));

    let mut m = baseline.clone();
    m.items[0].kind = PayeeKind::Treasury;
    mutations.push(("items[0].kind", m));

    let mut m = baseline.clone();
    m.items.swap(0, 1);
    mutations.push(("item order", m));

    let mut m = baseline.clone();
    m.items.pop();
    mutations.push(("item removed", m));

    for (field, mutated) in mutations {
        assert_ne!(
            mutated.compute_id(),
            baseline.id,
            "editing `{field}` left the plan id unchanged, so the hash does not cover it"
        );
        assert!(
            mutated.verify().is_err(),
            "a plan with `{field}` edited still verifies"
        );
    }
}

/// The id must depend on the outcome and nothing else, or two honest runs of
/// the same round disagree.
#[test]
fn incidental_fields_never_change_the_id() {
    let config = config_with(2);
    let baseline = plan_of(&config, &[3, 1], 1_000_000);

    let mut later = baseline.clone();
    later.created_at = 1_900_000_000;
    assert_eq!(later.compute_id(), baseline.id, "the clock must not matter");
    later.verify().expect("only the timestamp changed");

    let mut annotated = baseline.clone();
    annotated.items[0].handle = "renamed".into();
    annotated.items[0].score += 1;
    annotated.items[0].share_bps += 1;
    assert_eq!(
        annotated.compute_id(),
        baseline.id,
        "labels and display fields must not change where money goes"
    );
}

// ---------------------------------------------------------------------------
// conservation: money is never created or destroyed
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// FOUND: with no payable contributors the pool was absent from the plan
    /// entirely — 82.5% of a round vanished and `verify` accepted it.
    #[test]
    fn every_base_unit_is_assigned_or_declared(
        payable in 0usize..6,
        scores in prop::collection::vec(0u128..=1_000_000, 0..6),
        gross in 0u128..=1_000_000_000_000_000_000,
    ) {
        let config = config_with(payable);
        let plan = plan_of(&config, &scores, gross);

        let paid = plan.total().unwrap().base_units();
        prop_assert_eq!(
            paid + plan.undistributed.base_units(),
            gross,
            "transfers plus undistributed must equal the round exactly"
        );
        plan.verify().unwrap();
    }

    /// A round nobody can be paid from must state the whole pool as
    /// undistributed, not quietly shrink.
    #[test]
    fn a_round_with_no_payees_declares_the_whole_pool(gross in 1u128..=1_000_000_000_000) {
        let config = config_with(0);
        let plan = plan_of(&config, &[500], gross);

        prop_assert_eq!(plan.contributors().count(), 0);
        prop_assert_eq!(plan.undistributed, plan.split.contributors);
        prop_assert!(!plan.unresolved.is_empty(), "the earner must still be named");
        plan.verify().unwrap();
    }
}

// ---------------------------------------------------------------------------
// addresses: the one mistake that cannot be undone
// ---------------------------------------------------------------------------

/// FOUND: wallets were compared as strings, so EIP-55 capitalisation made one
/// contributor with two spellings of one address receive two transfers.
#[test]
fn one_account_spelled_many_ways_receives_one_transfer() {
    let body = "abcdef0000000000000000000000000000000001";
    let spellings = [
        format!("0x{body}"),
        format!("0X{}", body.to_uppercase()),
        format!(
            "0x{}",
            Address::parse(&format!("0x{body}"))
                .unwrap()
                .as_str()
                .trim_start_matches("0x")
        ),
    ];

    let mut config = config_with(0);
    config.identities = spellings
        .iter()
        .enumerate()
        .map(|(i, w)| {
            Identity::parse(format!("dev{i}"), w)
                .unwrap()
                .with_email(format!("dev{i}@example.com"))
        })
        .collect();

    let plan = plan_of(&config, &[1000, 1000, 1000], 1_000_000);

    assert_eq!(
        plan.contributors().count(),
        1,
        "three spellings of one account produced {} transfers",
        plan.contributors().count()
    );
    assert_eq!(
        plan.contributors().next().unwrap().amount,
        plan.split.contributors,
        "the merged payee must receive the whole pool"
    );
    plan.verify().unwrap();
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// The EIP-55 checksum exists to catch a slip of one character. If a
    /// mutated address still parses, a typo becomes an irreversible transfer.
    #[test]
    fn a_single_altered_character_is_rejected(
        seed in prop::array::uniform20(any::<u8>()),
        position in 0usize..40,
        replacement in 0u8..16,
    ) {
        let body: String = seed.iter().map(|b| format!("{b:02x}")).collect();
        let checksummed = Address::parse(&format!("0x{body}")).unwrap().to_string();
        let hex: Vec<char> = checksummed[2..].chars().collect();

        let new_char = char::from_digit(replacement as u32, 16).unwrap();
        // Only a genuine change counts; same digit in either case is not a typo.
        prop_assume!(!hex[position].eq_ignore_ascii_case(&new_char));

        let mut mutated = hex.clone();
        mutated[position] = new_char;
        let candidate: String = mutated.into_iter().collect();

        // A body that ends up all-one-case carries no checksum by definition.
        let letters: Vec<char> = candidate.chars().filter(|c| c.is_ascii_alphabetic()).collect();
        let uniform = letters.is_empty()
            || letters.iter().all(|c| c.is_ascii_lowercase())
            || letters.iter().all(|c| c.is_ascii_uppercase());
        prop_assume!(!uniform);

        prop_assert!(
            Address::parse(&format!("0x{candidate}")).is_err(),
            "changing position {position} of {checksummed} to `{new_char}` was accepted"
        );
    }

    /// Whatever anyone types, parsing decides — it never panics.
    #[test]
    fn address_parsing_never_panics(raw in ".{0,80}") {
        let _ = Address::parse(&raw);
    }
}

// ---------------------------------------------------------------------------
// identifiers must never become paths
// ---------------------------------------------------------------------------

/// FOUND: `plan_path` interpolated the id straight into a filename, so
/// `dedalo settle --plan ../../../../etc/passwd` read outside the ledger.
#[test]
fn a_plan_id_cannot_steer_the_filesystem() {
    let root = std::env::temp_dir().join(format!("dedalo-adv-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&root);
    let ledger = Ledger::open(&root).unwrap();

    let hostile = [
        "../../../../etc/passwd",
        "..",
        "a/b/c",
        "ded1/../../../etc/passwd",
        "ded1f643ee0b221a82c5b7fce39c04d0d591/../../escape",
        "/etc/passwd",
        "ded1f643ee0b221a82c5b7fce39c04d0d59",   // one short
        "ded1f643ee0b221a82c5b7fce39c04d0d5911", // one long
        "DED1F643EE0B221A82C5B7FCE39C04D0D591",  // wrong case
        "ded1f643ee0b221a82c5b7fce39c04d0d59g",  // not hex
        "ded1f643ee0b221a82c5b7fce39c04d0d591\0",
        "",
    ];
    for id in hostile {
        assert!(ledger.plan_path(id).is_err(), "plan_path accepted {id:?}");
        assert!(ledger.load_plan(id).is_err(), "load_plan accepted {id:?}");
    }

    // The real shape still works.
    let good = "ded1f643ee0b221a82c5b7fce39c04d0d591";
    let path = ledger.plan_path(good).expect("a real id must be accepted");
    assert!(
        path.starts_with(&root),
        "{} escaped {}",
        path.display(),
        root.display()
    );

    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// settlement: what must be refused
// ---------------------------------------------------------------------------

fn engine_for(repo: &TempRepo, config: Config) -> Engine {
    config.save(repo.path().join("dedalo.toml")).unwrap();
    let git = dedalo_core::git::CliGit::discover(repo.path()).unwrap();
    let ledger = Ledger::open(repo.path()).unwrap();
    Engine::new(
        config,
        repo.path().join("dedalo.toml"),
        Box::new(git),
        ledger,
    )
}

#[test]
fn settlement_refuses_the_zero_address() {
    let repo = TempRepo::new("adv-zero");
    repo.merge_feature("f", ("Ada", "ada@example.com"), 5);

    let mut config = config_with(1);
    config.identities[0].emails = vec!["ada@example.com".into()];
    config.wallets.treasury = Address::parse(ZERO_ADDRESS).unwrap();
    let engine = engine_for(&repo, config);

    let merges = engine.scan(None).unwrap();
    let attribution = engine.attribute(&merges);
    let plan = engine
        .plan(&merges, &attribution, Amount::from_base_units(1_000_000))
        .unwrap();

    let error = block_on(engine.settle(&plan, &DryRunSettlement::default()))
        .expect_err("the zero address must not be paid");
    let message = error.to_string();
    assert!(message.contains("zero address"), "{message}");
    assert!(
        message.contains("treasury"),
        "the refusal must name the payee: {message}"
    );
}

#[test]
fn settlement_refuses_a_round_that_reaches_nobody() {
    let repo = TempRepo::new("adv-nobody");
    repo.merge_feature("f", ("Ada", "ada@example.com"), 5);

    // Nobody linked, so the whole contributor pool has no destination.
    let engine = engine_for(&repo, config_with(0));
    let merges = engine.scan(None).unwrap();
    let attribution = engine.attribute(&merges);
    let plan = engine
        .plan(&merges, &attribution, Amount::from_base_units(1_000_000))
        .unwrap();
    assert!(!plan.undistributed.is_zero());

    let error = block_on(engine.settle(&plan, &DryRunSettlement::default()))
        .expect_err("a round that pays only fees must be refused by default");
    assert!(error.to_string().contains("no destination"), "{error}");

    // And goes through when the operator says they mean it.
    block_on(engine.settle_with(
        &plan,
        &DryRunSettlement::default(),
        &SettlementOptions::allowing_undistributed(),
    ))
    .expect("an explicit override must be honoured");
}

// ---------------------------------------------------------------------------
// hostile git history
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Commit messages, author names and emails are written by whoever opens a
    /// pull request. Scoring them must not panic, whatever they contain.
    #[test]
    fn attribution_survives_hostile_commit_metadata(
        name in ".{0,60}",
        email in ".{0,60}",
        subject in ".{0,120}",
        insertions in any::<u64>(),
        deletions in any::<u64>(),
    ) {
        let merge = MergeEvent {
            sha: "s".repeat(40),
            merged_by: Author::new(name.clone(), email.clone()),
            merged_at: i64::MIN,
            subject: subject.clone(),
            parents: vec!["p".into(), "q".into()],
            commits: vec![MergedCommit {
                sha: "c".repeat(40),
                author: Author::new(name, email),
                co_authors: vec![],
                authored_at: i64::MAX,
                subject,
            }],
            diff: DiffStat { files_changed: u64::MAX, insertions, deletions },
        };
        let attribution = Attribution::compute(&[merge], &AttributionPolicy::default());
        // Whatever it scores, the total must equal the sum of the parts.
        let sum: u128 = attribution.contributions.iter().map(|c| c.score).sum();
        prop_assert_eq!(sum, attribution.total_score);
    }

    /// A policy loaded from TOML must never make scoring wrap into a small
    /// number, which would silently rewrite everyone's share.
    #[test]
    fn extreme_policies_saturate_rather_than_wrap(
        base_points in any::<u64>(),
        per_insertion in 0.0f64..1e9,
        insertions in any::<u64>(),
    ) {
        let policy = AttributionPolicy {
            base_points,
            points_per_insertion: per_insertion,
            points_per_deletion: per_insertion,
            max_points_per_merge: 0,
            credit_merger: false,
            split_with_co_authors: true,
        };
        let merge = MergeEvent {
            sha: "s".repeat(40),
            merged_by: Author::new("m", "m@x"),
            merged_at: 0,
            subject: "Merge".into(),
            parents: vec!["p".into(), "q".into()],
            commits: vec![],
            diff: DiffStat { files_changed: 1, insertions, deletions: insertions },
        };
        // Monotonicity is the property wrapping destroys: a bigger diff can
        // never score lower than a smaller one. Saturation preserves it,
        // wrapping does not.
        let big = policy.merge_score(&merge);

        let mut smaller = merge.clone();
        smaller.diff.insertions = insertions / 2;
        smaller.diff.deletions = insertions / 2;
        let small = policy.merge_score(&smaller);

        prop_assert!(
            big >= small,
            "halving the diff raised the score: {small} > {big}, which means it wrapped"
        );
        prop_assert!(
            small >= (base_points as u128).saturating_mul(1_000),
            "the score fell below its own base points, which means it wrapped"
        );
    }
}

/// A config the loader accepts must never produce a wrapping score.
#[test]
fn absurd_weights_are_rejected_at_the_door() {
    for weight in [1e30, f64::INFINITY, f64::NAN, -1.0] {
        let mut config = config_with(1);
        config.attribution.points_per_insertion = weight;
        assert!(
            config.validate().is_err(),
            "a weight of {weight} was accepted into a config"
        );
    }
}

/// The core is runtime-agnostic; these tests poll its futures directly.
fn block_on<T>(future: impl Future<Output = T>) -> T {
    use std::pin::pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    const VTABLE: RawWakerVTable = RawWakerVTable::new(
        |_| RawWaker::new(std::ptr::null(), &VTABLE),
        |_| {},
        |_| {},
        |_| {},
    );
    let waker = unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) };
    let mut cx = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::hint::spin_loop(),
        }
    }
}
