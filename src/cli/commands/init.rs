//! `dedalo init` — write a starting `dedalo.toml`.

use crate::Config;
use crate::config::CONFIG_FILE;
use anyhow::{Context, Result, bail};

use crate::cli::args::InitArgs;
use crate::cli::commands::{display_path, workdir};
use crate::cli::ui;

/// Hand-written rather than serialized, so the file a maintainer opens
/// explains itself. Kept in sync with `Config` by `parses_as_a_valid_config`.
const TEMPLATE: &str = r##"# Dedalo — merge-to-earn funding rules for this repository.
# Everything here is public and reviewable: a payout can be recomputed by
# anyone from this file plus the git history.

[project]
name = "{{name}}"
# repository = "https://github.com/org/repo"
# Open Collective slug this project self-funds through.
{{open_collective}}

[git]
# Merges into this branch are what earn a payout.
branch = "{{branch}}"
# Merges whose subject starts with one of these are skipped entirely.
ignore_subjects = ["chore(release)", "Merge branch"]
# Emails that never receive a payout, however much they commit.
ignore_emails = ["noreply@github.com", "actions@github.com"]

[attribution]
# Flat score every merged pull request earns, regardless of size.
base_points = 100
# Per-line scoring. Deleting code is work too.
points_per_insertion = 1.0
points_per_deletion = 0.5
# Ceiling per merge, so one vendored dependency cannot drain a round.
max_points_per_merge = 5000
# Credit whoever pressed "merge", on top of the commit authors.
credit_merger = false
# Share a commit's score with its `Co-authored-by:` trailers.
split_with_co_authors = true

[asset]
# The token contributors are paid in.
symbol = "USDC"
decimals = 6
chain = "base"
contract = "So11111111111111111111111111111111111111112"

[fees]
# Taken off the top of every round, in basis points (10000 = 100%).
# The protocol fee funds the network's own Open Collective — this is what
# makes Dedalo self-sustaining instead of grant-dependent.
protocol_bps = 250
# Retained by this project for future rounds, audits, infrastructure.
treasury_bps = 1500
# The remaining 82.5% is split across contributors by attribution weight.

[wallets]
# Funds each round is paid out of.
source = "So11111111111111111111111111111111111111112"
# This project's own reserve.
treasury = "So11111111111111111111111111111111111111112"
# The network's Open Collective wallet, receiving `fees.protocol_bps`.
open_collective = "So11111111111111111111111111111111111111112"

[settlement]
# `dry-run` computes and verifies a round without spending anything.
# Switch to `evm` once the wallets above are real and a claim contract is
# deployed. Dedalo never signs: `dedalo propose` prints the transactions, and
# people execute them from the project's multisig.
backend = "dry-run"
# chain_id = 8453
# contract = "So11111111111111111111111111111111111111112"   # claim contract

# Contributors and their wallets. Add them with:
#   dedalo identity link <handle> <wallet> --email <git-email>
#
# [[identities]]
# handle = "ada"
# wallet = "So11111111111111111111111111111111111111112"
# emails = ["ada@example.com"]
"##;

pub fn run(args: &InitArgs, repo: Option<&std::path::PathBuf>, json: bool) -> Result<()> {
    let dir = workdir(repo)?;
    let root = git_root(&dir).unwrap_or(dir);
    let path = root.join(CONFIG_FILE);

    if path.exists() && !args.force {
        bail!(
            "{} already exists (use --force to overwrite)",
            display_path(&path)
        );
    }

    let name = args
        .name
        .clone()
        .or_else(|| root.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "my-project".to_string());

    let branch = default_branch(&root).unwrap_or_else(|| "main".to_string());
    let open_collective = match &args.open_collective {
        Some(slug) => format!("open_collective = \"{slug}\""),
        None => "# open_collective = \"my-project\"".to_string(),
    };

    let rendered = TEMPLATE
        .replace("{{name}}", &name)
        .replace("{{branch}}", &branch)
        .replace("{{open_collective}}", &open_collective);

    // Fail before writing if the template would not load back.
    let config: Config = toml::from_str(&rendered)
        .context("internal error: the init template is not a valid config")?;
    config.validate()?;

    std::fs::write(&path, &rendered)
        .with_context(|| format!("cannot write {}", display_path(&path)))?;

    if json {
        return crate::cli::commands::print_json(&serde_json::json!({
            "created": path,
            "project": name,
            "branch": branch,
        }));
    }

    println!("{} {}", ui::green("created"), display_path(&path));
    println!();
    println!("Next steps:");
    println!("  1. set the three addresses under [wallets]");
    println!("  2. dedalo identity link <handle> <wallet> --email <git-email>");
    println!(
        "  3. dedalo plan --amount 1000        {}",
        ui::dim("# preview a round")
    );
    Ok(())
}

fn git_root(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!path.is_empty()).then(|| std::path::PathBuf::from(path))
}

/// The branch whose merges should earn a payout.
///
/// The repository's default branch, not whatever happens to be checked out:
/// running `dedalo init` from a feature branch should not write that branch
/// into the config, where it would silently stop resolving the moment the
/// branch is merged and deleted.
fn default_branch(dir: &std::path::Path) -> Option<String> {
    let git = |args: &[&str]| -> Option<String> {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .ok()?;
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (output.status.success() && !value.is_empty()).then_some(value)
    };

    // What the remote says its default is.
    if let Some(head) = git(&["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
        && let Some(branch) = head.strip_prefix("origin/")
    {
        return Some(branch.to_string());
    }
    // Then the conventional names, if they exist here.
    for candidate in ["main", "master"] {
        if git(&["rev-parse", "--verify", "--quiet", candidate]).is_some() {
            return Some(candidate.to_string());
        }
    }
    // Failing both, whatever is checked out, as long as it is a branch.
    git(&["rev-parse", "--abbrev-ref", "HEAD"]).filter(|b| b != "HEAD")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_as_a_valid_config() {
        let rendered = TEMPLATE
            .replace("{{name}}", "dedalo")
            .replace("{{branch}}", "main")
            .replace("{{open_collective}}", "open_collective = \"dedalo\"");
        let config: Config = toml::from_str(&rendered).expect("template must parse");
        config.validate().expect("template must validate");
        assert_eq!(config.fees.contributor_bps(), 8_250);
    }
}
