//! What attribution costs on a repository that is not small.
//!
//! Nothing measured this, and the shape of the work says it should be measured
//! before somebody runs it on a big repository and finds out. `CliGit` drives
//! the `git` binary; `scan` reads every landed change on the branch since the
//! last settled commit, with the commits each one introduced and its diff
//! against the first parent. **On a first round there is no ledger, so "since
//! the last settled commit" means the whole history** — and that happens inside
//! a CI job with a timeout.
//!
//! These are `#[ignore]`d. Building ten thousand real merge commits takes
//! minutes, and a number that varies with what else the machine is doing is not
//! something to fail a build on. Run them deliberately:
//!
//! ```console
//! $ cargo test --release --all-features --test performance -- --ignored --nocapture
//! ```
//!
//! `--release` matters: the debug build spends its time in Dedalo rather than
//! in git, which measures the wrong thing.
//!
//! ## Why there is no regression gate
//!
//! A benchmark that fails the build on a 5% change is noise; one that fails on
//! a 5× change is a real guard. The measurements below say the cost is
//! dominated by `git` process spawns and diff computation, both of which vary
//! by more than 5% between runs on the same machine — so a tight gate would
//! fail for reasons that have nothing to do with this code.
//!
//! What *is* asserted is the shape rather than the constant:
//! [`the_cost_stays_linear_in_the_number_of_merges`] fails if the per-merge
//! cost grows with history size, which is what an accidental quadratic looks
//! like and is the regression actually worth catching.

use std::time::{Duration, Instant};

use dedalo::config::Config;
use dedalo::git::{CliGit, GitBackend, HistoryQuery, LandsAs};
use dedalo::money::Amount;
use dedalo::storage::ledger::Ledger;
use dedalo::testing::TempRepo;
use dedalo::{Engine, attribution::identity::Identity, chain::wallet::Address};

/// Build a repository with `merges` merge commits by four authors.
///
/// Four rather than one so identity resolution has something to do, and a
/// small line count per merge so the fixture measures the per-merge cost
/// rather than the cost of diffing large files.
fn repo_with(merges: usize) -> TempRepo {
    let repo = TempRepo::new(&format!("perf-{merges}"));
    let authors = [
        ("Ada", "ada@example.com"),
        ("Bea", "bea@example.com"),
        ("Cy", "cy@example.com"),
        ("Dee", "dee@example.com"),
    ];
    for index in 0..merges {
        repo.merge_feature(&format!("feature-{index}"), authors[index % 4], 8);
    }
    repo
}

fn engine_for(repo: &TempRepo) -> Engine {
    let mut config = Config::template("perf");
    config.project.open_collective = Some("perf-collective".into());
    config.wallets.source = Address::parse("So11111111111111111111111111111111111111112").unwrap();
    config.wallets.treasury =
        Address::parse("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
    config.wallets.open_collective =
        Address::parse("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL").unwrap();
    config.identities = vec![
        Identity::parse("ada", "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU")
            .unwrap()
            .with_email("ada@example.com"),
        Identity::parse("bea", "MerkS3LaQBSvM5JZsvBaLZBBSMvMB5aTuLRHrvKAyDo")
            .unwrap()
            .with_email("bea@example.com"),
    ];
    config.save(repo.path().join("dedalo.toml")).unwrap();

    let git = CliGit::discover(repo.path()).unwrap();
    let ledger = Ledger::open(repo.path()).unwrap();
    Engine::new(
        config,
        repo.path().join("dedalo.toml"),
        Box::new(git),
        ledger,
    )
}

/// Wall-clock cost of one full pipeline pass over `merges` merges.
struct Measurement {
    merges: usize,
    scan: Duration,
    attribute: Duration,
    plan: Duration,
    found: usize,
}

impl Measurement {
    /// Milliseconds per thousand merges, which is the number worth quoting.
    fn scan_per_thousand(&self) -> f64 {
        self.scan.as_secs_f64() * 1000.0 * 1000.0 / self.merges as f64
    }

    fn report(&self) {
        println!(
            "{:>6} merges | scan {:>8.0} ms | attribute {:>6.2} ms | plan {:>6.2} ms | \
             {:>7.0} ms per 1k merges | found {}",
            self.merges,
            self.scan.as_secs_f64() * 1000.0,
            self.attribute.as_secs_f64() * 1000.0,
            self.plan.as_secs_f64() * 1000.0,
            self.scan_per_thousand(),
            self.found,
        );
    }
}

fn measure(merges: usize) -> Measurement {
    let repo = repo_with(merges);
    let engine = engine_for(&repo);

    let start = Instant::now();
    let scanned = engine.scan(None).unwrap();
    let scan = start.elapsed();

    let start = Instant::now();
    let attribution = engine.attribute(&scanned);
    let attribute = start.elapsed();

    let start = Instant::now();
    let plan = engine
        .plan(&scanned, &attribution, Amount::from_base_units(1_000_000))
        .unwrap();
    let plan_time = start.elapsed();

    // A measurement of a pipeline that did not run is not a measurement.
    assert_eq!(scanned.len(), merges, "the fixture lost merges");
    assert!(!plan.items.is_empty(), "the plan paid nobody");

    Measurement {
        merges,
        scan,
        attribute,
        plan: plan_time,
        found: scanned.len(),
    }
}

/// Print the cost at three sizes, so the handbook can quote a real number.
#[test]
#[ignore = "builds thousands of real merge commits; run deliberately"]
fn what_a_round_costs_at_a_hundred_a_thousand_and_ten_thousand_merges() {
    println!();
    for merges in [100, 1_000, 10_000] {
        measure(merges).report();
    }
    println!();
    println!("Stages 2 and 3 are pure arithmetic over what stage 1 read.");
    println!("If `scan` dominates, the cost is git, not attribution.");
}

/// The regression worth catching is a change of shape, not of constant.
///
/// Reading history is one `git log` invocation plus one diff per change, so
/// the cost per merge should not depend on how many merges came before. If it
/// starts to, something has become quadratic — a per-merge call that walks the
/// whole history, most likely — and that is the failure that turns a ten
/// minute CI job into one that never finishes.
///
/// The threshold is deliberately loose. Process spawn time and diff cost vary
/// by tens of percent between runs on the same machine; 3× does not.
#[test]
#[ignore = "builds thousands of real merge commits; run deliberately"]
fn the_cost_stays_linear_in_the_number_of_merges() {
    let small = measure(200);
    let large = measure(2_000);

    small.report();
    large.report();

    let ratio = large.scan_per_thousand() / small.scan_per_thousand();
    println!("per-merge cost at 2000 merges is {ratio:.2}× the cost at 200");

    assert!(
        ratio < 3.0,
        "the cost per merge grew {ratio:.2}× between 200 and 2000 merges, \
         which is the shape of an accidental quadratic rather than of noise"
    );
}

/// `--limit` must reach `git log`, not just the printed table.
///
/// It used to truncate *after* reading everything: `scan` asked for the whole
/// history, computed a diff for every merge, and then threw away all but the
/// last few. On a repository with ten thousand merges, `dedalo scan --limit 10`
/// did ten thousand merges' worth of work to show ten rows.
///
/// This asserts the property that fix has to keep: a bounded read is
/// dramatically cheaper than an unbounded one over the same history.
#[test]
#[ignore = "builds thousands of real merge commits; run deliberately"]
fn a_limited_scan_does_not_read_the_whole_history() {
    let repo = repo_with(1_000);
    let engine = engine_for(&repo);

    let start = Instant::now();
    let all = engine.scan(None).unwrap();
    let unbounded = start.elapsed();

    let start = Instant::now();
    let recent = engine.scan_recent(None, Some(10)).unwrap();
    let bounded = start.elapsed();

    println!(
        "1000 merges: unbounded {:.0} ms for {} merges, limited {:.0} ms for {}",
        unbounded.as_secs_f64() * 1000.0,
        all.len(),
        bounded.as_secs_f64() * 1000.0,
        recent.len(),
    );

    assert_eq!(all.len(), 1_000);
    assert!(
        recent.len() <= 10,
        "a limit of 10 returned {} merges",
        recent.len()
    );
    assert!(
        bounded * 5 < unbounded,
        "a limited scan took {bounded:?} against {unbounded:?} unbounded, which \
         means the limit is still being applied after the work rather than before it"
    );
}

/// The limit reaches the query object, which is the mechanism the test above
/// measures. Cheap enough to run on every build, so a refactor that stops
/// passing it fails immediately rather than at the next benchmark run.
#[test]
fn the_history_query_carries_the_limit() {
    let repo = TempRepo::new("limit");
    for index in 0..6 {
        repo.merge_feature(&format!("feature-{index}"), ("Ada", "ada@example.com"), 4);
    }

    let git = CliGit::discover(repo.path()).unwrap();
    let query = HistoryQuery {
        branch: "main".into(),
        since_commit: None,
        since_timestamp: None,
        limit: Some(2),
        lands_as: LandsAs::Merges,
    };

    let bounded = git.merges(&query).unwrap();
    assert_eq!(bounded.len(), 2, "git log ignored --max-count");

    let unbounded = git.merges(&HistoryQuery {
        limit: None,
        ..query
    });
    assert_eq!(unbounded.unwrap().len(), 6);
}
