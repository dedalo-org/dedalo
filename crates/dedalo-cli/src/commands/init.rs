//! `dedalo init` — write a starting `dedalo.toml`.

use anyhow::{Context, Result, bail};
use dedalo_core::Config;
use dedalo_core::config::CONFIG_FILE;

use crate::cli::InitArgs;
use crate::commands::{display_path, workdir};
use crate::ui;

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
contract = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913"

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
source = "0x0000000000000000000000000000000000000000"
# This project's own reserve.
treasury = "0x0000000000000000000000000000000000000000"
# The network's Open Collective wallet, receiving `fees.protocol_bps`.
open_collective = "0x0000000000000000000000000000000000000000"

[settlement]
# `dry-run` computes and verifies a round without spending anything.
# Switch to `evm` once the wallets above are real.
backend = "dry-run"
# rpc_url = "https://mainnet.base.org"
# chain_id = 8453
# contract = "0x0000000000000000000000000000000000000000"
# Env var holding the signing key. The key itself never goes in this file.
signer_env = "DEDALO_SIGNER_KEY"

# Contributors and their wallets. Add them with:
#   dedalo identity link <handle> <wallet> --email <git-email>
#
# [[identities]]
# handle = "ada"
# wallet = "0x0000000000000000000000000000000000000000"
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

    let branch = current_branch(&root).unwrap_or_else(|| "main".to_string());
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
        return crate::commands::print_json(&serde_json::json!({
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

fn current_branch(dir: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && !branch.is_empty() && branch != "HEAD").then_some(branch)
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
