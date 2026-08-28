//! Turning merge history into contribution weights.
//!
//! The scoring rule is intentionally simple and fully specified by config:
//! two people running `dedalo plan` on the same commit range must produce
//! identical weights. Scores are integers (milli-points) so no floating point
//! rounding can drift between machines.

pub mod identity;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::git::{Author, MergeEvent};

/// How much a merge is worth, and to whom.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AttributionPolicy {
    /// Flat score every merged pull request earns, regardless of size.
    pub base_points: u64,
    /// Score per inserted line.
    pub points_per_insertion: f64,
    /// Score per deleted line. Deleting code is work too, usually good work.
    pub points_per_deletion: f64,
    /// Ceiling per merge, so one vendored dependency cannot drain a round.
    pub max_points_per_merge: u64,
    /// Credit whoever merged, on top of the commit authors.
    pub credit_merger: bool,
    /// Share a commit's score equally with its `Co-authored-by:` trailers.
    pub split_with_co_authors: bool,
}

impl Default for AttributionPolicy {
    fn default() -> Self {
        Self {
            base_points: 100,
            points_per_insertion: 1.0,
            points_per_deletion: 0.5,
            max_points_per_merge: 5_000,
            credit_merger: false,
            split_with_co_authors: true,
        }
    }
}

/// Internal fixed-point scale: 1 point == 1000 milli-points.
const MILLI: u128 = 1_000;

impl AttributionPolicy {
    fn milli(value: f64) -> u128 {
        if !value.is_finite() || value <= 0.0 {
            return 0;
        }
        (value * MILLI as f64).round() as u128
    }

    /// Largest weight a single line may carry, in points.
    ///
    /// The config takes a float, and TOML will happily hold `1e300`. Without
    /// a ceiling, a large diff multiplied by a large weight overflows `u128`
    /// — panicking in a debug build and, worse, wrapping to a small number in
    /// a release one, which silently rewrites everyone's share.
    pub const MAX_LINE_WEIGHT: f64 = 1_000_000.0;

    /// Total milli-points a merge is worth, before splitting between people.
    ///
    /// Saturating rather than wrapping: a score that has hit the ceiling is
    /// wrong, but it is visibly, boringly wrong. A wrapped one looks
    /// plausible and pays the wrong people.
    pub fn merge_score(&self, merge: &MergeEvent) -> u128 {
        let raw = (self.base_points as u128)
            .saturating_mul(MILLI)
            .saturating_add(
                (merge.diff.insertions as u128)
                    .saturating_mul(Self::milli(self.points_per_insertion)),
            )
            .saturating_add(
                (merge.diff.deletions as u128)
                    .saturating_mul(Self::milli(self.points_per_deletion)),
            );
        let cap = (self.max_points_per_merge as u128).saturating_mul(MILLI);
        if cap == 0 { raw } else { raw.min(cap) }
    }

    /// Reject weights that would make scoring meaningless or unstable.
    pub fn validate(&self) -> Result<()> {
        for (name, value) in [
            ("points_per_insertion", self.points_per_insertion),
            ("points_per_deletion", self.points_per_deletion),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(Error::config(format!(
                    "attribution.{name} must be a finite, non-negative number (found {value})"
                )));
            }
            if value > Self::MAX_LINE_WEIGHT {
                return Err(Error::config(format!(
                    "attribution.{name} is {value}, above the {} ceiling; a weight that \
                     large makes a single merge outweigh every other",
                    Self::MAX_LINE_WEIGHT
                )));
            }
        }
        Ok(())
    }
}

/// Everything one contributor earned over the analysed range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contribution {
    /// The git identity that earned this, before wallet resolution.
    pub author: Author,
    /// Weight in milli-points.
    pub score: u128,
    /// Merges this person took part in.
    pub merges: u64,
    /// Commits credited to this person, co-authored ones included.
    pub commits: u64,
    /// Lines added by the merges they took part in.
    pub insertions: u64,
    /// Lines removed by the merges they took part in.
    pub deletions: u64,
}

/// Scores for a commit range, keyed by lowercased author email.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Attribution {
    /// Contributors, highest score first.
    pub contributions: Vec<Contribution>,
    /// How many merges produced these scores.
    pub merges_analysed: u64,
    /// Sum of every contributor's score, the denominator of a split.
    pub total_score: u128,
}

impl Attribution {
    /// Score a slice of merge history under `policy`.
    ///
    /// Each merge's score is split evenly across the commits it introduced,
    /// then each commit's slice is split evenly across its author and any
    /// co-authors. A merge with no introduced commits (a squash landed
    /// directly, or an empty merge) credits the merger, so work is never lost.
    pub fn compute(merges: &[MergeEvent], policy: &AttributionPolicy) -> Self {
        let mut acc: BTreeMap<String, Contribution> = BTreeMap::new();
        let mut total_score = 0u128;

        for merge in merges {
            let merge_score = policy.merge_score(merge);
            let mut credited: Vec<(Author, u128)> = Vec::new();

            if merge.commits.is_empty() {
                credited.push((merge.merged_by.clone(), merge_score));
            } else {
                let per_commit = merge_score / merge.commits.len() as u128;
                // Any remainder from the division goes to the first commit,
                // keeping the sum exact.
                let mut remainder = merge_score - per_commit * merge.commits.len() as u128;
                for commit in &merge.commits {
                    let mut share = per_commit;
                    if remainder > 0 {
                        share += remainder;
                        remainder = 0;
                    }
                    let mut people = vec![commit.author.clone()];
                    if policy.split_with_co_authors {
                        people.extend(commit.co_authors.iter().cloned());
                    }
                    let each = share / people.len() as u128;
                    let mut left = share - each * people.len() as u128;
                    for person in people {
                        let mut value = each;
                        if left > 0 {
                            value += left;
                            left = 0;
                        }
                        credited.push((person, value));
                    }
                }
            }

            if policy.credit_merger && !merge.commits.is_empty() {
                credited.push((merge.merged_by.clone(), merge_score));
            }

            for (author, score) in credited {
                total_score += score;
                let entry = acc.entry(author.key()).or_insert_with(|| Contribution {
                    author: author.clone(),
                    score: 0,
                    merges: 0,
                    commits: 0,
                    insertions: 0,
                    deletions: 0,
                });
                entry.score += score;
                entry.commits += 1;
            }

            // Count each merge once per distinct participant.
            for key in participants(merge, policy) {
                if let Some(entry) = acc.get_mut(&key) {
                    entry.merges += 1;
                    entry.insertions += merge.diff.insertions;
                    entry.deletions += merge.diff.deletions;
                }
            }
        }

        let mut contributions: Vec<Contribution> = acc.into_values().collect();
        // Highest earner first; email keeps ties stable across machines.
        contributions.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.author.key().cmp(&b.author.key()))
        });

        Self {
            contributions,
            merges_analysed: merges.len() as u64,
            total_score,
        }
    }

    /// Whether there is anything to pay out at all.
    pub fn is_empty(&self) -> bool {
        self.contributions.is_empty() || self.total_score == 0
    }

    /// Scores in contributor order, ready for [`crate::money::Amount::split_by_weights`].
    pub fn weights(&self) -> Vec<u128> {
        self.contributions.iter().map(|c| c.score).collect()
    }

    /// Share of the contributor pool, in basis points, for display only.
    pub fn share_bps(&self, contribution: &Contribution) -> u32 {
        if self.total_score == 0 {
            return 0;
        }
        ((contribution.score * 10_000) / self.total_score) as u32
    }
}

fn participants(merge: &MergeEvent, policy: &AttributionPolicy) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    let push = |author: &Author, keys: &mut Vec<String>| {
        let key = author.key();
        if !keys.contains(&key) {
            keys.push(key);
        }
    };
    if merge.commits.is_empty() || policy.credit_merger {
        push(&merge.merged_by, &mut keys);
    }
    for commit in &merge.commits {
        push(&commit.author, &mut keys);
        if policy.split_with_co_authors {
            for co in &commit.co_authors {
                push(co, &mut keys);
            }
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{DiffStat, MergedCommit};

    fn commit(email: &str, co: &[&str]) -> MergedCommit {
        MergedCommit {
            sha: format!("sha-{email}"),
            author: Author::new(email, email),
            co_authors: co.iter().map(|e| Author::new(*e, *e)).collect(),
            authored_at: 0,
            subject: "work".into(),
        }
    }

    fn merge(commits: Vec<MergedCommit>, insertions: u64) -> MergeEvent {
        MergeEvent {
            sha: "merge".repeat(8),
            merged_by: Author::new("maint", "maint@example.com"),
            merged_at: 0,
            subject: "Merge PR".into(),
            parents: vec!["p1".into(), "p2".into()],
            commits,
            diff: DiffStat {
                files_changed: 1,
                insertions,
                deletions: 0,
            },
        }
    }

    #[test]
    fn splits_merge_score_across_commits_and_co_authors() {
        let policy = AttributionPolicy {
            base_points: 100,
            points_per_insertion: 1.0,
            points_per_deletion: 0.0,
            max_points_per_merge: 0,
            credit_merger: false,
            split_with_co_authors: true,
        };
        // 100 base + 100 insertions = 200 points, one commit, two people.
        let attribution =
            Attribution::compute(&[merge(vec![commit("a@x.io", &["b@x.io"])], 100)], &policy);
        assert_eq!(attribution.total_score, 200 * MILLI);
        assert_eq!(attribution.contributions.len(), 2);
        assert_eq!(attribution.contributions[0].score, 100 * MILLI);
        assert_eq!(attribution.contributions[1].score, 100 * MILLI);
    }

    #[test]
    fn caps_oversized_merges() {
        let policy = AttributionPolicy {
            max_points_per_merge: 500,
            ..AttributionPolicy::default()
        };
        let attribution =
            Attribution::compute(&[merge(vec![commit("a@x.io", &[])], 1_000_000)], &policy);
        assert_eq!(attribution.total_score, 500 * MILLI);
    }

    #[test]
    fn empty_merge_credits_the_merger() {
        let attribution = Attribution::compute(&[merge(vec![], 10)], &AttributionPolicy::default());
        assert_eq!(attribution.contributions.len(), 1);
        assert_eq!(
            attribution.contributions[0].author.email,
            "maint@example.com"
        );
    }

    #[test]
    fn score_is_conserved_when_splitting() {
        let policy = AttributionPolicy {
            base_points: 100,
            points_per_insertion: 0.0,
            points_per_deletion: 0.0,
            max_points_per_merge: 0,
            credit_merger: false,
            split_with_co_authors: true,
        };
        // 100_000 milli-points over 3 commits does not divide evenly.
        let commits = vec![
            commit("a@x.io", &[]),
            commit("b@x.io", &[]),
            commit("c@x.io", &[]),
        ];
        let attribution = Attribution::compute(&[merge(commits, 0)], &policy);
        let sum: u128 = attribution.contributions.iter().map(|c| c.score).sum();
        assert_eq!(sum, 100 * MILLI);
        assert_eq!(attribution.total_score, 100 * MILLI);
    }

    /// `credit_merger` pays whoever pressed the button, on top of the authors.
    ///
    /// It is off by default and it is a rule about who gets money, so it needs
    /// a test that says what turning it on does — and it had none. The merger
    /// earns the *whole* merge score again rather than a share of it, which is
    /// the surprising part and the reason to write it down.
    #[test]
    fn crediting_the_merger_pays_them_on_top_of_the_authors() {
        let mut policy = AttributionPolicy {
            credit_merger: true,
            ..AttributionPolicy::default()
        };
        policy.base_points = 100;
        policy.points_per_insertion = 0.0;
        policy.points_per_deletion = 0.0;

        let commits = vec![commit("ada@x.io", &[])];
        let mut event = merge(commits, 0);
        event.merged_by = Author::new("Maintainer", "maint@x.io");

        let attribution = Attribution::compute(&[event.clone()], &policy);
        let score_of = |email: &str| {
            attribution
                .contributions
                .iter()
                .find(|c| c.author.email == email)
                .unwrap_or_else(|| panic!("{email} earned nothing"))
                .score
        };

        assert_eq!(score_of("ada@x.io"), 100 * MILLI);
        assert_eq!(score_of("maint@x.io"), 100 * MILLI);
        assert_eq!(attribution.total_score, 200 * MILLI);

        // Off by default, and off means off: the same merge pays the author
        // alone.
        let policy = AttributionPolicy {
            credit_merger: false,
            ..policy
        };
        let attribution = Attribution::compute(&[event], &policy);
        assert_eq!(attribution.contributions.len(), 1);
        assert_eq!(attribution.total_score, 100 * MILLI);
    }

    /// An empty round divides by nothing and says zero rather than panicking.
    ///
    /// `weights()` is what `split_by_weights` is handed, so its ordering is
    /// load-bearing: the share a contributor is paid is the weight at their
    /// index. Nothing checked that the two orders are the same one.
    #[test]
    fn weights_follow_contributor_order_and_an_empty_round_is_zero_share() {
        let policy = AttributionPolicy::default();
        let commits = vec![commit("ada@x.io", &[]), commit("bea@x.io", &[])];
        let attribution = Attribution::compute(&[merge(commits, 0)], &policy);

        let weights = attribution.weights();
        assert_eq!(weights.len(), attribution.contributions.len());
        for (contribution, weight) in attribution.contributions.iter().zip(&weights) {
            assert_eq!(contribution.score, *weight, "weights are out of order");
        }

        let empty = Attribution::default();
        assert!(empty.is_empty());
        assert!(empty.weights().is_empty());
        // The one input that cannot be divided by.
        let contribution = &attribution.contributions[0];
        assert_eq!(empty.share_bps(contribution), 0);
    }
}
