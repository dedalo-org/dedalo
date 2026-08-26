//! [`GitBackend`] implementation that drives the `git` executable.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{Author, DiffStat, GitBackend, HistoryQuery, LandsAs, MergeEvent, MergedCommit};
use crate::error::{Error, Result};

/// ASCII unit/record separators: safe field delimiters because git refuses
/// them in ref names and they are vanishingly rare in commit messages.
const FIELD: char = '\x1f';
const RECORD: char = '\x1e';

/// A repository read through the `git` executable.
#[derive(Debug, Clone)]
pub struct CliGit {
    root: PathBuf,
}

impl CliGit {
    /// Open the repository containing `path`, walking up to the work tree root.
    pub fn discover(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let out = run(path, &["rev-parse", "--show-toplevel"])?;
        Ok(Self {
            root: PathBuf::from(out.trim()),
        })
    }

    /// Open a repository at an exact work tree root, without discovery.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn git(&self, args: &[&str]) -> Result<String> {
        run(&self.root, args)
    }

    /// Diff of a merge against its mainline parent: what the merge really added.
    fn merge_diff(&self, merge_sha: &str, first_parent: &str) -> Result<DiffStat> {
        let range = format!("{first_parent}..{merge_sha}");
        let out = self.git(&["diff", "--numstat", "--no-renames", &range])?;
        let mut stat = DiffStat::default();
        for line in out.lines().filter(|l| !l.trim().is_empty()) {
            let mut parts = line.split('\t');
            let added = parts.next().unwrap_or("0");
            let removed = parts.next().unwrap_or("0");
            // Binary files report "-": counted as a changed file, zero lines.
            stat.insertions += added.parse::<u64>().unwrap_or(0);
            stat.deletions += removed.parse::<u64>().unwrap_or(0);
            stat.files_changed += 1;
        }
        Ok(stat)
    }

    /// Commits introduced by `merge_sha`, i.e. reachable from the merged branch
    /// but not from the mainline it landed on.
    ///
    /// A change with one parent introduced exactly itself, and the caller
    /// builds that record from fields it already read. Returning nothing here
    /// would score a squashed pull request as having contributed no commits,
    /// which is how `split_with_co_authors` would quietly stop working.
    fn merged_commits(&self, parents: &[String]) -> Result<Vec<MergedCommit>> {
        let (Some(first), Some(second)) = (parents.first(), parents.get(1)) else {
            return Ok(Vec::new());
        };
        let range = format!("{first}..{second}");
        let format = format!("%H{FIELD}%an{FIELD}%ae{FIELD}%at{FIELD}%s{FIELD}%b{RECORD}");
        let out = self.git(&["log", &format!("--format={format}"), "--no-merges", &range])?;

        let mut commits = Vec::new();
        for record in out.split(RECORD) {
            let record = record.trim_start_matches(['\n', '\r']);
            if record.trim().is_empty() {
                continue;
            }
            let fields: Vec<&str> = record.split(FIELD).collect();
            if fields.len() < 6 {
                return Err(Error::GitParse {
                    context: format!("commits of {range}"),
                    detail: format!("expected 6 fields, got {}", fields.len()),
                });
            }
            commits.push(MergedCommit {
                sha: fields[0].trim().to_string(),
                author: Author::new(fields[1], fields[2]),
                co_authors: parse_co_authors(fields[5]),
                authored_at: fields[3].trim().parse().unwrap_or_default(),
                subject: fields[4].trim().to_string(),
            });
        }
        // git log is newest-first; contributions read better oldest-first.
        commits.reverse();
        Ok(commits)
    }
}

impl GitBackend for CliGit {
    fn root(&self) -> &Path {
        &self.root
    }

    fn current_branch(&self) -> Result<String> {
        Ok(self
            .git(&["rev-parse", "--abbrev-ref", "HEAD"])?
            .trim()
            .to_string())
    }

    fn resolve(&self, rev: &str) -> Result<String> {
        Ok(self.git(&["rev-parse", rev])?.trim().to_string())
    }

    fn merges(&self, query: &HistoryQuery) -> Result<Vec<MergeEvent>> {
        // Author and committer are both read because they differ exactly
        // where it matters. On a squashed pull request the author is whoever
        // wrote the work and the committer is whatever landed it — usually
        // `GitHub <noreply@github.com>`, which `ignore_emails` already drops.
        // Crediting the author as the merger would pay the same person twice
        // wherever `credit_merger` is on.
        //
        // `%b` is last, so a body containing a separator can only corrupt
        // itself.
        let format = format!(
            "%H{FIELD}%P{FIELD}%an{FIELD}%ae{FIELD}%at{FIELD}\
             %cn{FIELD}%ce{FIELD}%ct{FIELD}%s{FIELD}%b{RECORD}"
        );
        let format_arg = format!("--format={format}");

        // `--first-parent` keeps us on the mainline: work inside a feature
        // branch is already accounted for by whatever landed it.
        let mut args: Vec<String> = vec!["log".into(), format_arg, "--first-parent".into()];
        if query.lands_as == LandsAs::Merges {
            args.push("--merges".into());
        }
        if let Some(limit) = query.limit {
            args.push(format!("--max-count={limit}"));
        }
        if let Some(ts) = query.since_timestamp {
            args.push(format!("--since={ts}"));
        }
        let range = match &query.since_commit {
            Some(since) => format!("{since}..{}", query.branch),
            None => query.branch.clone(),
        };
        args.push(range);

        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = self.git(&borrowed)?;

        let mut merges = Vec::new();
        for record in out.split(RECORD) {
            let record = record.trim_start_matches(['\n', '\r']);
            if record.trim().is_empty() {
                continue;
            }
            let fields: Vec<&str> = record.split(FIELD).collect();
            if fields.len() < 10 {
                return Err(Error::GitParse {
                    context: "history of landed changes".into(),
                    detail: format!("expected 10 fields, got {}", fields.len()),
                });
            }
            let sha = fields[0].trim().to_string();
            let parents: Vec<String> = fields[1].split_whitespace().map(str::to_string).collect();
            let author = Author::new(fields[2], fields[3]);
            let authored_at: i64 = fields[4].trim().parse().unwrap_or_default();
            let committer = Author::new(fields[5], fields[6]);
            let landed_at: i64 = fields[7].trim().parse().unwrap_or_default();
            let subject = fields[8].trim().to_string();
            let diff = match parents.first() {
                Some(first) => self.merge_diff(&sha, first)?,
                None => DiffStat::default(),
            };
            // A merge carries the work on its second parent. Anything else
            // introduced exactly itself, and every field for that record has
            // already been read.
            let is_merge = parents.len() > 1;
            let commits = if is_merge {
                self.merged_commits(&parents)?
            } else {
                vec![MergedCommit {
                    sha: sha.clone(),
                    author: author.clone(),
                    co_authors: parse_co_authors(fields[9]),
                    authored_at,
                    subject: subject.clone(),
                }]
            };
            merges.push(MergeEvent {
                commits,
                sha,
                // A merge commit's author is whoever produced the merge, which
                // is what this field has always meant. A squashed pull request
                // has no such person, so the committer is the closest true
                // answer to "what landed this".
                merged_by: if is_merge { author } else { committer },
                merged_at: landed_at,
                subject,
                parents,
                diff,
            });
        }
        merges.reverse();
        Ok(merges)
    }

    fn has_merge_commits(&self, branch: &str) -> Result<bool> {
        let out = self.git(&[
            "log",
            "--merges",
            "--first-parent",
            "--max-count=1",
            "--format=%H",
            branch,
        ])?;
        Ok(!out.trim().is_empty())
    }
}

fn run(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::GitMissing(e)
            } else {
                Error::io(cwd, e)
            }
        })?;

    if !output.status.success() {
        return Err(Error::Git {
            args: args.join(" "),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Extract `Co-authored-by: Name <email>` trailers from a commit body.
fn parse_co_authors(body: &str) -> Vec<Author> {
    let mut out = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        let Some(rest) = strip_prefix_ci(line, "co-authored-by:") else {
            continue;
        };
        // Kept as nested `if`s rather than a let-chain so the crate still
        // builds on its declared minimum supported Rust version.
        if let Some(author) = parse_author_line(rest.trim())
            && !out.contains(&author)
        {
            out.push(author);
        }
    }
    out
}

fn strip_prefix_ci<'a>(haystack: &'a str, prefix: &str) -> Option<&'a str> {
    // `get` rather than an index: slicing by byte length panics when a
    // multibyte character straddles the boundary, and commit messages are
    // full of em-dashes, accents and emoji. Every prefix here is ASCII, so a
    // slice that is not on a boundary could never have matched anyway.
    let head = haystack.get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix) {
        Some(&haystack[prefix.len()..])
    } else {
        None
    }
}

fn parse_author_line(value: &str) -> Option<Author> {
    let open = value.find('<')?;
    let close = value.rfind('>')?;
    if close <= open {
        return None;
    }
    let name = value[..open].trim();
    let email = value[open + 1..close].trim();
    if email.is_empty() {
        return None;
    }
    Some(Author::new(name, email))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// This parser reads whatever anyone wrote in a commit message. It is
        /// allowed to find nothing; it is never allowed to panic.
        #[test]
        fn never_panics_on_arbitrary_commit_bodies(body in ".{0,200}") {
            let _ = parse_co_authors(&body);
        }

        /// Same for the author line itself, trailer prefix already stripped.
        #[test]
        fn never_panics_on_arbitrary_author_lines(line in ".{0,120}") {
            let _ = parse_author_line(&line);
        }
    }

    #[test]
    fn reads_co_author_trailers_case_insensitively() {
        let body = "Fix the parser\n\nCo-authored-by: Ada <ada@example.com>\nco-authored-by: Bea <bea@example.com>\n";
        let authors = parse_co_authors(body);
        assert_eq!(authors.len(), 2);
        assert_eq!(authors[0].email, "ada@example.com");
        assert_eq!(authors[1].name, "Bea");
    }

    /// Commit messages are prose. A character that straddles the byte length
    /// of the trailer prefix used to panic the whole scan.
    #[test]
    fn survives_multibyte_characters_in_a_commit_body() {
        for body in [
            "would reject \u{2014} but only after the fact.",
            "caf\u{e9}",
            "\u{1f980} rust",
            "\u{2014}",
            "Co-authored-by: \u{c9}va <eva@example.com>",
            "co-authored-by: \u{1f680} <rocket@example.com>",
        ] {
            let authors = parse_co_authors(body);
            // Only the two real trailers carry an author.
            if body.to_ascii_lowercase().starts_with("co-authored-by:") {
                assert_eq!(authors.len(), 1, "failed on {body:?}");
            } else {
                assert!(authors.is_empty(), "failed on {body:?}");
            }
        }
    }

    #[test]
    fn ignores_malformed_trailers_and_duplicates() {
        let body = "Co-authored-by: nobody\nCo-authored-by: Ada <ada@example.com>\nCo-authored-by: Ada <ada@example.com>";
        assert_eq!(parse_co_authors(body).len(), 1);
    }
}
