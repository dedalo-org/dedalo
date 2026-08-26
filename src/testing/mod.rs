//! Throwaway repositories for tests.
//!
//! Dedalo's behaviour is defined by what git actually reports, so testing it
//! against a mock would test the mock. [`TempRepo`] builds a real repository
//! with real merge commits, and removes it when it drops.
//!
//! Enable with the `testing` feature:
//!
//! ```toml
//! [dev-dependencies]
//! dedalo = { version = "0.0.1", features = ["testing"] }
//! ```
//!
//! ```no_run
//! use dedalo::testing::TempRepo;
//!
//! let repo = TempRepo::new("example");
//! repo.merge_feature("feature-a", ("Ada", "ada@example.com"), 40);
//! // `repo.path()` is now a repository with one merge on `main`.
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

/// Distinguishes repositories created in the same process and millisecond.
static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A git repository in a temporary directory, deleted on drop.
///
/// Every commit is made with fixed committer details and signing disabled, so
/// the tests behave the same on a machine with a personalised git config.
#[derive(Debug)]
pub struct TempRepo {
    path: PathBuf,
}

impl TempRepo {
    /// Create a repository with one initial commit on `main`.
    ///
    /// # Panics
    ///
    /// Panics if `git` is not installed or the repository cannot be created.
    /// Tests cannot meaningfully continue past either.
    pub fn new(tag: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let path = std::env::temp_dir().join(format!(
            "dedalo-test-{tag}-{}-{unique}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("cannot create the temporary directory");

        let repo = Self { path };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.name", "Maintainer"]);
        repo.git(&["config", "user.email", "maint@example.com"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo.git(&["config", "tag.gpgsign", "false"]);
        // Windows runners would otherwise rewrite line endings and change the
        // diff sizes attribution is scored on.
        repo.git(&["config", "core.autocrlf", "false"]);
        repo.commit_file("README.md", "seed\n", "Initial commit", None);
        repo
    }

    /// The repository's working tree root.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Run a git command in the repository and return its stdout.
    ///
    /// # Panics
    ///
    /// Panics if git exits non-zero, with its stderr in the message.
    pub fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.path)
            .args(args)
            .env("GIT_COMMITTER_NAME", "Maintainer")
            .env("GIT_COMMITTER_EMAIL", "maint@example.com")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("git must be installed to use crate::testing");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Write a file and commit it, optionally as a different author.
    pub fn commit_file(&self, name: &str, body: &str, message: &str, author: Option<(&str, &str)>) {
        let target = self.path.join(name);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).expect("cannot create the parent directory");
        }
        std::fs::write(&target, body).expect("cannot write the file");
        self.git(&["add", "-A"]);

        let (name, email) = author.unwrap_or(("Maintainer", "maint@example.com"));
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.path)
            .args(["commit", "-q", "-m", message])
            .env("GIT_AUTHOR_NAME", name)
            .env("GIT_AUTHOR_EMAIL", email)
            .env("GIT_COMMITTER_NAME", "Maintainer")
            .env("GIT_COMMITTER_EMAIL", "maint@example.com")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("git must be installed to use crate::testing");
        assert!(
            output.status.success(),
            "commit failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Branch off `main`, add `lines` lines authored by `author`, and merge back.
    ///
    /// This is the shape Dedalo pays for: a merge commit whose second parent
    /// carries someone else's work.
    pub fn merge_feature(&self, branch: &str, author: (&str, &str), lines: usize) {
        self.merge_feature_with_trailer(branch, author, lines, None);
    }

    /// Like [`TempRepo::merge_feature`], with a trailer on the feature commit.
    ///
    /// Use it to exercise `Co-authored-by:` handling.
    pub fn merge_feature_with_trailer(
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

    /// Full hash of the current `HEAD`.
    pub fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"]).trim().to_string()
    }

    /// Write a file at the repository root without committing it.
    pub fn write(&self, name: &str, contents: &str) {
        std::fs::write(self.path.join(name), contents).expect("cannot write the file");
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        // A leaked temp directory is a nuisance, not a test failure.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
