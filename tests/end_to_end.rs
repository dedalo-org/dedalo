//! End-to-end test over a real git repository.
//!
//! The unit tests cover the arithmetic; this one covers the part that can
//! only break against the actual `git` binary: reading merges, attributing
//! them, and turning them into a settled round. The repository harness lives
//! in `dedalo::testing`, so downstream crates can use the same one.

use dedalo::config::Config;
use dedalo::git::{CliGit, GitBackend, HistoryQuery};
use dedalo::identity::Identity;
use dedalo::ledger::Ledger;
use dedalo::money::Amount;
use dedalo::settlement::DryRunSettlement;
use dedalo::testing::TempRepo;
use dedalo::wallet::Address;
use dedalo::{Engine, payout::PayeeKind};

fn config_for(repo: &TempRepo) -> Config {
    let mut config = Config::template("demo");
    config.project.open_collective = Some("demo-collective".into());
    config.wallets.source = Address::parse("0x1111111111111111111111111111111111111111").unwrap();
    config.wallets.treasury = Address::parse("0x2222222222222222222222222222222222222222").unwrap();
    config.wallets.open_collective =
        Address::parse("0x3333333333333333333333333333333333333333").unwrap();
    config.identities = vec![
        Identity::parse("ada", "0x00000000000000000000000000000000000000ad")
            .unwrap()
            .with_email("ada@example.com"),
        Identity::parse("bea", "0x00000000000000000000000000000000000000be")
            .unwrap()
            .with_email("bea@example.com"),
    ];
    config.save(repo.path().join("dedalo.toml")).unwrap();
    config
}

fn engine_for(repo: &TempRepo) -> Engine {
    let config = config_for(repo);
    let git = CliGit::discover(repo.path()).unwrap();
    let ledger = Ledger::open(repo.path()).unwrap();
    Engine::new(
        config,
        repo.path().join("dedalo.toml"),
        Box::new(git),
        ledger,
    )
}

#[test]
fn reads_merges_with_their_commits_and_diffs() {
    let repo = TempRepo::new("read");
    repo.merge_feature("feature-a", ("Ada", "ada@example.com"), 40);
    repo.merge_feature_with_trailer(
        "feature-b",
        ("Bea", "bea@example.com"),
        10,
        Some("Co-authored-by: Cy <cy@example.com>"),
    );

    let git = CliGit::discover(repo.path()).unwrap();
    let merges = git
        .merges(&HistoryQuery {
            branch: "main".into(),
            ..HistoryQuery::default()
        })
        .unwrap();

    assert_eq!(merges.len(), 2);
    // Oldest first, so a plan reads like the project's timeline.
    assert!(merges[0].subject.contains("feature-a"));
    assert_eq!(merges[0].commits.len(), 1);
    assert_eq!(merges[0].commits[0].author.email, "ada@example.com");
    assert_eq!(merges[0].diff.insertions, 40);
    assert_eq!(merges[0].diff.files_changed, 1);

    // Co-authors are read off the merged commit, not the merge commit.
    assert_eq!(merges[1].commits[0].co_authors.len(), 1);
    assert_eq!(merges[1].commits[0].co_authors[0].email, "cy@example.com");
    // The merge itself was made by the maintainer, who authored nothing.
    assert_eq!(merges[1].merged_by.email, "maint@example.com");
}

#[test]
fn full_round_pays_contributors_treasury_and_protocol() {
    let repo = TempRepo::new("round");
    repo.merge_feature("feature-a", ("Ada", "ada@example.com"), 30);
    repo.merge_feature("feature-b", ("Bea", "bea@example.com"), 10);

    let engine = engine_for(&repo);
    let merges = engine.scan(None).unwrap();
    assert_eq!(merges.len(), 2);

    let attribution = engine.attribute(&merges);
    assert_eq!(attribution.contributions.len(), 2);

    let gross = engine.config().asset.parse_amount("1000").unwrap();
    let plan = engine.plan(&merges, &attribution, gross).unwrap();
    plan.verify().unwrap();

    // Nothing is created or lost: every base unit is assigned to someone.
    assert_eq!(plan.total().unwrap(), gross);

    let protocol = plan
        .items
        .iter()
        .find(|item| item.kind == PayeeKind::Protocol)
        .unwrap();
    assert_eq!(
        protocol.wallet,
        Address::parse("0x3333333333333333333333333333333333333333").unwrap()
    );
    assert_eq!(protocol.amount, gross.bps(250).unwrap());

    // Ada wrote three times the lines Bea did, so she earns more.
    let ada = plan.items.iter().find(|i| i.handle == "ada").unwrap();
    let bea = plan.items.iter().find(|i| i.handle == "bea").unwrap();
    assert!(ada.amount > bea.amount);

    let receipt = futures_block_on(engine.settle(&plan, &DryRunSettlement::default())).unwrap();
    assert!(receipt.dry_run);
    // A simulated round must not move the cursor.
    assert!(engine.state().unwrap().last_settled_commit.is_none());
}

#[test]
fn scanning_resumes_after_the_last_settled_commit() {
    let repo = TempRepo::new("cursor");
    repo.merge_feature("feature-a", ("Ada", "ada@example.com"), 10);
    let first_round_head = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    let engine = engine_for(&repo);
    assert_eq!(engine.scan(None).unwrap().len(), 1);

    repo.merge_feature("feature-b", ("Bea", "bea@example.com"), 10);
    assert_eq!(engine.scan(None).unwrap().len(), 2);

    // Once the first merge is paid for, only newer merges remain pending.
    let pending = engine.scan(Some(&first_round_head)).unwrap();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].subject.contains("feature-b"));
}

/// CI checks out a detached HEAD without creating local branches, so `main`
/// does not resolve even though `origin/main` does. A tool that calls itself
/// CI-native has to work there.
#[test]
fn a_detached_checkout_resolves_the_branch_through_the_remote() {
    let origin = TempRepo::new("origin");
    origin.merge_feature("feature-a", ("Ada", "ada@example.com"), 10);

    // A clone, then the state actions/checkout leaves behind: detached, with
    // no local branch of the name the config asks for.
    let clone = TempRepo::new("detached");
    clone.git(&[
        "remote",
        "add",
        "upstream",
        &origin.path().to_string_lossy(),
    ]);
    clone.git(&["fetch", "--quiet", "upstream"]);
    clone.git(&[
        "update-ref",
        "refs/remotes/origin/main",
        "refs/remotes/upstream/main",
    ]);
    clone.git(&[
        "checkout",
        "--quiet",
        "--detach",
        "refs/remotes/origin/main",
    ]);
    clone.git(&["branch", "-D", "main"]);
    assert!(
        std::process::Command::new("git")
            .arg("-C")
            .arg(clone.path())
            .args(["rev-parse", "--verify", "--quiet", "refs/heads/main"])
            .output()
            .unwrap()
            .stdout
            .is_empty(),
        "the fixture must have no local `main`"
    );

    let engine = engine_for(&clone);
    let merges = engine.scan(None).expect("must resolve through origin/main");
    assert_eq!(merges.len(), 1);

    // The plan still records the configured branch name, not the ref it
    // resolved to — otherwise the same history would yield two plan ids.
    let attribution = engine.attribute(&merges);
    let plan = engine
        .plan(&merges, &attribution, Amount::from_base_units(1_000))
        .unwrap();
    assert_eq!(plan.range.branch, "main");
}

#[test]
fn a_branch_that_exists_nowhere_says_so() {
    let repo = TempRepo::new("nobranch");
    repo.merge_feature("feature-a", ("Ada", "ada@example.com"), 5);

    let mut config = config_for(&repo);
    config.git.branch = "does-not-exist".into();
    config.save(repo.path().join("dedalo.toml")).unwrap();

    let git = CliGit::discover(repo.path()).unwrap();
    let ledger = Ledger::open(repo.path()).unwrap();
    let engine = Engine::new(
        config,
        repo.path().join("dedalo.toml"),
        Box::new(git),
        ledger,
    );

    let error = engine
        .scan(None)
        .expect_err("an absent branch must be an error");
    let message = error.to_string();
    assert!(message.contains("does-not-exist"), "{message}");
    assert!(message.contains("origin/does-not-exist"), "{message}");
    assert!(
        message.contains("fetch-depth"),
        "the message must say how to fix it: {message}"
    );
}

#[test]
fn the_same_history_always_produces_the_same_plan_id() {
    let repo = TempRepo::new("determinism");
    repo.merge_feature("feature-a", ("Ada", "ada@example.com"), 25);

    let engine = engine_for(&repo);
    let merges = engine.scan(None).unwrap();
    let attribution = engine.attribute(&merges);
    let gross = Amount::from_base_units(1_000_000);

    let first = engine.plan(&merges, &attribution, gross).unwrap();
    let second = engine.plan(&merges, &attribution, gross).unwrap();
    assert_eq!(first.id, second.id);

    // A different round size is a different plan.
    let bigger = engine
        .plan(&merges, &attribution, Amount::from_base_units(2_000_000))
        .unwrap();
    assert_ne!(first.id, bigger.id);
}

/// The core crate is runtime-agnostic; tests drive its futures directly.
fn futures_block_on<T>(future: impl Future<Output = T>) -> T {
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
