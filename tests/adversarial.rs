//! Adversarial tests: what the system must refuse.
//!
//! The rest of the suite asks whether Dedalo computes the right answer. This
//! one asks whether it can be made to compute a wrong one — by a hostile
//! input, a mistyped address, a race, or a plan edited after review. Every
//! test here corresponds to a way money could actually be lost.
//!
//! Each one that carries a `FOUND:` note is a regression test for a defect
//! that was real in this codebase, not a hypothetical.

use dedalo::attribution::identity::Identity;
use dedalo::attribution::{Attribution, AttributionPolicy, Contribution};
use dedalo::chain::settlement::DryRunSettlement;
use dedalo::chain::wallet::{Address, ZERO_ADDRESS};
use dedalo::config::Config;
use dedalo::git::{Author, DiffStat, MergeEvent, MergedCommit};
use dedalo::money::{Amount, Asset};
use dedalo::payout::{PayeeKind, PayoutPlan, PlanBuilder, PlanRange};
use dedalo::storage::ledger::Ledger;
use dedalo::testing::TempRepo;
use dedalo::{Engine, SettlementOptions};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------------

/// A distinct, on-curve address for a fixture.
///
/// Built from bytes rather than written out, so a fixture cannot accidentally
/// be two names for one account — which is the defect
/// `one_account_listed_many_times_receives_one_transfer` exists to catch and
/// would be embarrassing to reintroduce in the fixtures themselves.
fn address(tag: u8) -> Address {
    let mut raw = [tag; 32];
    for nudge in 0..=u8::MAX {
        raw[31] = nudge;
        let candidate = Address::from_pubkey_bytes(raw);
        if candidate.is_on_curve() {
            return candidate;
        }
    }
    unreachable!("some nudge of the last byte lands on the curve")
}

fn config_with(contributors: usize) -> Config {
    let mut config = Config::template("adversarial");
    config.asset = Asset::native("TEST", "testnet", 6);
    config.wallets.source = address(1);
    config.wallets.treasury = address(2);
    config.wallets.open_collective = address(3);
    config.identities = (0..contributors)
        .map(|i| {
            Identity::parse(format!("dev{i}"), &address((i + 0xa0) as u8).to_string())
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
    m.asset.contract = Some(address(7).to_string());
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
    m.items[0].wallet = address(9);
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

/// FOUND: wallets were compared as strings, so one contributor with two
/// spellings of one address received two transfers.
///
/// base58 has one encoding per value, so an account has exactly one spelling
/// and the original defect cannot recur. What replaced it is the mirror image,
/// below.
#[test]
fn one_account_listed_many_times_receives_one_transfer() {
    let wallet = "So11111111111111111111111111111111111111112";

    let mut config = config_with(0);
    config.identities = (0..3)
        .map(|i| {
            Identity::parse(format!("dev{i}"), wallet)
                .unwrap()
                .with_email(format!("dev{i}@example.com"))
        })
        .collect();

    let plan = plan_of(&config, &[1000, 1000, 1000], 1_000_000);

    assert_eq!(
        plan.contributors().count(),
        1,
        "three identities on one wallet produced {} transfers",
        plan.contributors().count()
    );
    assert_eq!(
        plan.contributors().next().unwrap().amount,
        plan.split.contributors,
        "the merged payee must receive the whole pool"
    );
    plan.verify().unwrap();
}

/// The mirror of the defect above, and the one that would be introduced by
/// carrying the EVM's habits across.
///
/// EIP-55 put a checksum in an address's capitalisation, so comparison there
/// had to fold case. base58 is case-**sensitive**: two strings differing only
/// in case are two unrelated accounts. Folding case here would silently pay
/// one person twice and the other never.
#[test]
fn two_accounts_differing_only_in_case_are_not_merged() {
    // Searched for rather than hardcoded, so this keeps testing the property
    // and not one lucky pair.
    let mut raw = [0u8; 32];
    let mut found = None;
    for byte in 0..=u8::MAX {
        raw[0] = byte;
        let candidate = Address::from_pubkey_bytes(raw).to_string();
        let flipped: String = candidate
            .chars()
            .map(|c| {
                if c.is_ascii_uppercase() {
                    c.to_ascii_lowercase()
                } else {
                    c.to_ascii_uppercase()
                }
            })
            .collect();
        if flipped == candidate {
            continue;
        }
        if let Ok(other) = Address::parse(&flipped) {
            found = Some((Address::parse(&candidate).unwrap(), other));
            break;
        }
    }

    let (a, b) = found.expect("some address flips case into another valid address");
    assert_ne!(a.as_str(), b.as_str());
    assert_eq!(
        a.as_str().to_ascii_lowercase(),
        b.as_str().to_ascii_lowercase(),
        "the pair must be indistinguishable once case is folded"
    );
    assert_ne!(a.key(), b.key(), "folding case would merge two accounts");
    assert_ne!(a, b, "and they must not compare equal");
}

/// base58 decoding, implemented here from the definition.
///
/// The point of a second implementation is that the properties below compare
/// the crate against the encoding rather than against itself. A test that asks
/// the code under test what the right answer is proves nothing.
fn base58_decode(value: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

    let mut bytes: Vec<u8> = vec![0];
    for c in value.chars() {
        let digit = ALPHABET.iter().position(|a| *a as char == c)? as u32;
        // bytes = bytes * 58 + digit, big-endian, by hand.
        let mut carry = digit;
        for byte in bytes.iter_mut().rev() {
            let value = u32::from(*byte) * 58 + carry;
            *byte = (value & 0xff) as u8;
            carry = value >> 8;
        }
        while carry > 0 {
            bytes.insert(0, (carry & 0xff) as u8);
            carry >>= 8;
        }
    }

    // Each leading `1` is one leading zero byte.
    let leading = value.chars().take_while(|c| *c == '1').count();
    while bytes.first() == Some(&0) {
        bytes.remove(0);
    }
    let mut out = vec![0u8; leading];
    out.extend(bytes);
    Some(out)
}

/// The measurement this project owes anyone who reads "addresses are
/// validated".
///
/// Under EIP-55 a mistyped address was usually rejected — around fifteen bits
/// of checksum hidden in the capitalisation, and the pinned counterexamples
/// here were the *exceptions*. **A Solana address has no checksum at all.**
/// Every thirty-two byte value is a valid key, so a slip is caught only when
/// it changes the decoded length.
///
/// So this test does not pin exceptions. It measures the rule, and asserts the
/// number is bad, because a number nobody has looked at is how "validated"
/// turns into a word people trust.
#[test]
fn most_single_character_slips_produce_another_valid_address() {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

    let original = "So11111111111111111111111111111111111111112";
    let chars: Vec<char> = original.chars().collect();

    let mut tried = 0usize;
    let mut accepted = 0usize;
    for position in 0..chars.len() {
        for replacement in ALPHABET {
            let replacement = *replacement as char;
            if replacement == chars[position] {
                continue;
            }
            let mut mutated = chars.clone();
            mutated[position] = replacement;
            let candidate: String = mutated.into_iter().collect();
            tried += 1;
            if let Ok(parsed) = Address::parse(&candidate) {
                accepted += 1;
                assert_ne!(
                    parsed.as_str(),
                    original,
                    "a mutation must not decode back to the original"
                );
            }
        }
    }

    let survived = accepted * 100 / tried;
    assert!(
        survived > 50,
        "expected most slips to survive — the point of this test is that they \
         do. {accepted} of {tried} ({survived}%)"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Validity is decided by the encoding alone, and this crate must agree
    /// with an independent decoder in both directions.
    ///
    /// "Usually rejected" would pass while a validator quietly accepted
    /// everything, and "always rejected" is false — there is nothing to reject
    /// a well-formed address for. Agreeing with a second implementation is the
    /// strongest true statement available.
    #[test]
    fn an_address_is_accepted_exactly_when_it_decodes_to_thirty_two_bytes(
        seed in prop::array::uniform32(any::<u8>()),
        position in 0usize..32,
        replacement in 0u8..58,
    ) {
        const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

        let original = Address::from_pubkey_bytes(seed).to_string();
        let chars: Vec<char> = original.chars().collect();
        prop_assume!(position < chars.len());

        let new_char = ALPHABET[replacement as usize] as char;
        prop_assume!(new_char != chars[position]);

        let mut mutated = chars.clone();
        mutated[position] = new_char;
        let candidate: String = mutated.into_iter().collect();

        let decoded = base58_decode(&candidate);
        let should_parse = decoded.as_ref().is_some_and(|d| d.len() == 32);

        prop_assert_eq!(
            Address::parse(&candidate).is_ok(),
            should_parse,
            "changing position {} of {} to `{}`: independent decode says 32 bytes = {}",
            position, original, new_char, should_parse
        );
    }

    /// The independent decoder and the crate agree on what the bytes are, not
    /// merely on whether they exist.
    #[test]
    fn the_crate_and_an_independent_decoder_read_the_same_bytes(
        seed in prop::array::uniform32(any::<u8>()),
    ) {
        let address = Address::from_pubkey_bytes(seed);
        let decoded = base58_decode(address.as_str()).expect("a written address decodes");
        prop_assert_eq!(&decoded[..], &seed[..]);
        prop_assert_eq!(address.pubkey_bytes().unwrap(), seed);
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
    let git = dedalo::git::CliGit::discover(repo.path()).unwrap();
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
