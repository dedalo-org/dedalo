//! End-to-end test over a real git repository.
//!
//! The unit tests cover the arithmetic; this one covers the part that can
//! only break against the actual `git` binary: reading merges, attributing
//! them, and turning them into a settled round.

use std::path::{Path, PathBuf};
use std::process::Command;

use dedalo_core::config::Config;
use dedalo_core::git::{CliGit, GitBackend, HistoryQuery};
use dedalo_core::identity::Identity;
use dedalo_core::ledger::Ledger;
use dedalo_core::money::Amount;
use dedalo_core::settlement::DryRunSettlement;
use dedalo_core::{Engine, payout::PayeeKind};

/// A throwaway repository that cleans itself up.
struct TempRepo {
    path: PathBuf,
}

impl TempRepo {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "dedalo-e2e-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        let repo = Self { path };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.name", "Maintainer"]);
        repo.git(&["config", "user.email", "maint@example.com"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo.commit_file("README.md", "seed", "Initial commit", None);
        repo
    }

    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.path)
            .args(args)
            .env("GIT_COMMITTER_NAME", "Maintainer")
            .env("GIT_COMMITTER_EMAIL", "maint@example.com")
            .output()
            .expect("git must be installed to run these tests");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn commit_file(&self, name: &str, body: &str, message: &str, author: Option<(&str, &str)>) {
        std::fs::write(self.path.join(name), body).unwrap();
        self.git(&["add", "-A"]);
        let (author_name, author_email) = author.unwrap_or(("Maintainer", "maint@example.com"));
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.path)
            .args(["commit", "-q", "-m", message])
            .env("GIT_AUTHOR_NAME", author_name)
            .env("GIT_AUTHOR_EMAIL", author_email)
            .env("GIT_COMMITTER_NAME", "Maintainer")
            .env("GIT_COMMITTER_EMAIL", "maint@example.com")
            .output()
            .unwrap();
        assert!(output.status.success());
    }

    /// Branch off main, add `lines` lines authored by `author`, merge back.
    fn merge_feature(
        &self,
        branch: &str,
        author: (&str, &str),
        lines: usize,
        trailer: Option<&str>,
    ) {
        self.git(&["checkout", "-q", "-b", branch, "main"]);
        let body: String = (0..lines).map(|i| format!("line {i}\n")).collect();
        let message = match trailer {
            Some(trailer) => format!("Implement {branch}\n\n{trailer}"),
            None => format!("Implement {branch}"),
        };
        self.commit_file(&format!("{branch}.txt"), &body, &message, Some(author));
        self.git(&["checkout", "-q", "main"]);
        self.git(&[
            "merge",
            "-q",
            "--no-ff",
            branch,
            "-m",
            &format!("Merge pull request: {branch}"),
        ]);
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn config_for(repo: &TempRepo) -> Config {
    let mut config = Config::template("demo");
    config.project.open_collective = Some("demo-collective".into());
    config.wallets.source = "0xsource".into();
    config.wallets.treasury = "0xtreasury".into();
    config.wallets.open_collective = "0xopencollective".into();
    config.identities = vec![
        Identity::new("ada", "0xada").with_email("ada@example.com"),
        Identity::new("bea", "0xbea").with_email("bea@example.com"),
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
    repo.merge_feature("feature-a", ("Ada", "ada@example.com"), 40, None);
    repo.merge_feature(
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
    repo.merge_feature("feature-a", ("Ada", "ada@example.com"), 30, None);
    repo.merge_feature("feature-b", ("Bea", "bea@example.com"), 10, None);

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
    assert_eq!(protocol.wallet, "0xopencollective");
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
    repo.merge_feature("feature-a", ("Ada", "ada@example.com"), 10, None);
    let first_round_head = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    let engine = engine_for(&repo);
    assert_eq!(engine.scan(None).unwrap().len(), 1);

    repo.merge_feature("feature-b", ("Bea", "bea@example.com"), 10, None);
    assert_eq!(engine.scan(None).unwrap().len(), 2);

    // Once the first merge is paid for, only newer merges remain pending.
    let pending = engine.scan(Some(&first_round_head)).unwrap();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].subject.contains("feature-b"));
}

#[test]
fn the_same_history_always_produces_the_same_plan_id() {
    let repo = TempRepo::new("determinism");
    repo.merge_feature("feature-a", ("Ada", "ada@example.com"), 25, None);

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
