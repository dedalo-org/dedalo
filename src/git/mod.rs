//! Git is the source of truth.
//!
//! Dedalo does not keep its own database of "who did what": it derives every
//! payout from changes that are already in the repository, so any third party
//! can recompute a plan from the same history and get the same numbers.
//!
//! # What counts as a landed change
//!
//! A merge commit was the original answer, and it is wrong for most of
//! GitHub. Squash-and-merge — the default many projects pick — produces one
//! ordinary commit with a single parent, and rebase-and-merge produces
//! several. On such a repository `--merges` matches nothing, on a history
//! full of merged work.
//!
//! So the unit is a [`LandsAs`] choice, and it is a config setting rather
//! than a guess: it decides what people are paid for, and a tool that silently
//! changes its mind about that between runs is worse than one that asks.
//! [`LandsAs::Merges`] stays the default, because changing what an existing
//! project pays for must be somebody's decision and not an upgrade.

mod process;

pub use process::CliGit;

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// A git author as recorded in commit metadata.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Author {
    /// Display name as written in the commit.
    pub name: String,
    /// Email, the key an identity is matched on.
    pub email: String,
}

impl Author {
    /// Build an author from a name and email.
    pub fn new(name: impl Into<String>, email: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            email: email.into(),
        }
    }

    /// Lowercased email, the key identities are matched on.
    pub fn key(&self) -> String {
        self.email.trim().to_ascii_lowercase()
    }
}

impl std::fmt::Display for Author {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} <{}>", self.name, self.email)
    }
}

/// One commit brought into the mainline by a merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergedCommit {
    /// Full commit hash.
    pub sha: String,
    /// Whoever wrote it.
    pub author: Author,
    /// `Co-authored-by:` trailers, which split credit with the author.
    #[serde(default)]
    pub co_authors: Vec<Author>,
    /// Author date, as a unix timestamp.
    pub authored_at: i64,
    /// First line of the commit message.
    pub subject: String,
}

/// Aggregated diff size of a merge, measured against its first parent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffStat {
    /// Files touched, binary ones included.
    pub files_changed: u64,
    /// Lines added.
    pub insertions: u64,
    /// Lines removed.
    pub deletions: u64,
}

/// A change that landed on the tracked branch: the unit Dedalo pays for.
///
/// Named for the merge commit it was first modelled on. Under
/// [`LandsAs::Commits`] it is an ordinary commit instead, and then `sha` is
/// that commit, `parents` has one entry, and `commits` holds the change
/// itself rather than what a merge dragged in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeEvent {
    /// Hash of the merge commit itself.
    pub sha: String,
    /// Whoever pressed the merge button. Credited only if configured to be.
    pub merged_by: Author,
    /// Commit date of the merge, as a unix timestamp.
    pub merged_at: i64,
    /// First line of the merge commit message.
    pub subject: String,
    /// Parent hashes; the first is the mainline.
    pub parents: Vec<String>,
    /// Commits this merge introduced, i.e. `first_parent..second_parent`.
    pub commits: Vec<MergedCommit>,
    /// Size of the merge measured against its first parent.
    pub diff: DiffStat,
}

impl MergeEvent {
    /// Abbreviated hash, for terminal output.
    pub fn short_sha(&self) -> &str {
        &self.sha[..self.sha.len().min(8)]
    }
}

/// Selects which slice of history to read.
#[derive(Debug, Clone, Default)]
pub struct HistoryQuery {
    /// Branch or ref to follow, e.g. `main`.
    pub branch: String,
    /// Exclusive lower bound: only merges *after* this commit are returned.
    pub since_commit: Option<String>,
    /// Inclusive lower bound as a unix timestamp.
    pub since_timestamp: Option<i64>,
    /// Hard cap on how many changes to return, newest-first before ordering.
    pub limit: Option<usize>,
    /// What shape a landed change has in this repository.
    pub lands_as: LandsAs,
}

/// How a change lands on the tracked branch in this repository.
///
/// Not a detection. Which one is right depends on a setting in the forge —
/// whether the merge button squashes — and getting it wrong changes what
/// every contributor is paid, so it is written in `dedalo.toml` where it can
/// be reviewed like any other funding rule.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LandsAs {
    /// A merge commit, with the work on its second parent.
    ///
    /// The default, and the only safe one to keep: a project already paying
    /// for merges must not start paying for something else because it
    /// upgraded.
    #[default]
    Merges,
    /// Every commit on the branch's first-parent line.
    ///
    /// What a squash-merge repository produces, where one pull request is one
    /// ordinary commit. A merge commit on such a branch still counts, and
    /// still brings in the commits on its second parent, so a history that
    /// mixes both is not counted twice.
    ///
    /// This pays for *everything* on the branch, including a direct push. On
    /// a branch that requires pull requests there is no difference; on one
    /// that does not, that is the trade being made.
    Commits,
}

impl LandsAs {
    /// How to describe this to somebody reading terminal output.
    pub fn description(self) -> &'static str {
        match self {
            LandsAs::Merges => "merge commits",
            LandsAs::Commits => "every commit on the branch",
        }
    }
}

/// Read-only access to a repository's history of landed changes.
///
/// The default implementation shells out to the `git` binary ([`CliGit`]),
/// which keeps the dependency tree small and behaves identically to what a
/// maintainer sees in their terminal. Swapping in a linked-library backend
/// later only requires another impl of this trait.
pub trait GitBackend: Send + Sync {
    /// Absolute path of the repository working tree.
    fn root(&self) -> &std::path::Path;

    /// Currently checked out branch name.
    fn current_branch(&self) -> Result<String>;

    /// Resolve a revision (branch, tag, sha) to a full commit sha.
    fn resolve(&self, rev: &str) -> Result<String>;

    /// Every landed change matching `query`, ordered oldest to newest.
    fn merges(&self, query: &HistoryQuery) -> Result<Vec<MergeEvent>>;

    /// Whether `branch` contains a merge commit at all.
    ///
    /// Separate from [`GitBackend::merges`] because an empty result has two
    /// very different causes — nothing new since the last round, or a
    /// repository whose merge button squashes — and telling somebody the
    /// wrong one costs them every payout they never noticed was missing.
    fn has_merge_commits(&self, branch: &str) -> Result<bool>;
}
