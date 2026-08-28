//! `dedalo scan` and `dedalo contributors` — read-only views of pending work.

use crate::Engine;
use crate::git::LandsAs;
use anyhow::Result;

use crate::cli::args::RangeArgs;
use crate::cli::ui::{self, Align, Table};

pub fn scan(engine: &Engine, args: &RangeArgs, json: bool) -> Result<()> {
    // The limit goes into the query rather than being applied to the result.
    // Truncating afterwards meant `--limit 10` computed a diff for every merge
    // in the repository and then discarded all but ten — on a first round,
    // with no ledger, that is the entire history.
    let merges = engine.scan_recent(args.since.as_deref(), args.limit)?;

    if json {
        return crate::cli::commands::print_json(&merges);
    }

    if merges.is_empty() {
        return report_nothing_pending(engine);
    }

    let policy = &engine.config().attribution;
    // The column and the count name whatever this repository actually pays
    // for, so the output cannot imply merge commits to a project that has
    // none.
    let unit = match engine.config().git.lands_as {
        LandsAs::Merges => ("MERGE", "merges"),
        LandsAs::Commits => ("CHANGE", "changes"),
    };
    let mut table = Table::new(&[
        (unit.0, Align::Left),
        ("DATE", Align::Left),
        ("COMMITS", Align::Right),
        ("+/-", Align::Right),
        ("SCORE", Align::Right),
        ("SUBJECT", Align::Left),
    ]);
    for merge in &merges {
        table.push(vec![
            merge.short_sha().to_string(),
            ui::format_timestamp(merge.merged_at),
            merge.commits.len().to_string(),
            format!("+{} -{}", merge.diff.insertions, merge.diff.deletions),
            ui::format_score(policy.merge_score(merge)),
            ui::truncate(&merge.subject, 48),
        ]);
    }
    print!("{}", table.render());
    println!();
    // With a limit in effect, how many are pending is a question this command
    // deliberately did not do the work to answer. Saying "10 merges pending"
    // because ten were read would be a confident wrong answer about how much a
    // round owes.
    if args.limit.is_some() {
        println!(
            "showing the {} most recent {} on {} — pass no --limit for the whole range",
            ui::bold(&merges.len().to_string()),
            unit.1,
            engine.config().git.branch
        );
    } else {
        println!(
            "{} {} pending on {}",
            ui::bold(&merges.len().to_string()),
            unit.1,
            engine.config().git.branch
        );
    }
    Ok(())
}

/// Explain an empty scan, which has two causes that look identical.
///
/// "Nothing new since the last round" and "this repository's merge button
/// squashes, so there has never been anything to find" produce the same empty
/// table. Reporting the first when it is the second is how a project goes on
/// paying nobody while the tool appears to work — so the branch is asked
/// whether it contains a merge commit at all before anything is claimed.
fn report_nothing_pending(engine: &Engine) -> Result<()> {
    let branch = &engine.config().git.branch;

    if engine.config().git.lands_as == LandsAs::Commits {
        println!("No unpaid changes on {}.", ui::bold(branch));
        return Ok(());
    }

    // Only `merges` mode can be looking for the wrong thing.
    if engine.repo().has_merge_commits(branch)? {
        println!("No unpaid merges on {}.", ui::bold(branch));
        return Ok(());
    }

    println!("No unpaid merges on {}.", ui::bold(branch));
    println!();
    println!(
        "{} {} contains no merge commit at all, so there is nothing here to \n\
         pay for and never will be. Squash-and-merge and rebase-and-merge both \n\
         land a pull request as an ordinary commit.",
        ui::bold("Note:"),
        branch,
    );
    println!();
    println!("If that is how this project merges, say so in dedalo.toml:");
    println!();
    println!("    [git]");
    println!("    lands_as = \"commits\"");
    println!();
    println!(
        "That pays for every commit on {}'s first-parent line. On a branch \n\
         that requires pull requests those are the same thing; on one that \n\
         does not, a direct push earns too.",
        branch,
    );
    Ok(())
}

pub fn contributors(engine: &Engine, args: &RangeArgs, json: bool) -> Result<()> {
    let merges = engine.scan(args.since.as_deref())?;
    let attribution = engine.attribute(&merges);

    if json {
        return crate::cli::commands::print_json(&attribution);
    }

    if attribution.is_empty() {
        println!("No contributions in the pending range.");
        return Ok(());
    }

    let identities = engine.config().identity_map();
    let mut table = Table::new(&[
        ("CONTRIBUTOR", Align::Left),
        ("WALLET", Align::Left),
        ("MERGES", Align::Right),
        ("+/-", Align::Right),
        ("SCORE", Align::Right),
        ("SHARE", Align::Right),
    ]);

    let limit = args.limit.unwrap_or(usize::MAX);
    for contribution in attribution.contributions.iter().take(limit) {
        let wallet = match identities.resolve(&contribution.author) {
            Some(identity) if identity.excluded => ui::dim("excluded"),
            Some(identity) => identity
                .wallet
                .as_ref()
                .map_or_else(|| ui::yellow("no wallet"), |w| ui::truncate(w.as_str(), 12)),
            None => ui::yellow("no wallet"),
        };
        table.push(vec![
            ui::truncate(&contribution.author.name, 24),
            wallet,
            contribution.merges.to_string(),
            format!("+{} -{}", contribution.insertions, contribution.deletions),
            ui::format_score(contribution.score),
            ui::format_bps(attribution.share_bps(contribution)),
        ]);
    }
    print!("{}", table.render());
    println!();
    println!(
        "{} contributors across {} merges",
        ui::bold(&attribution.contributions.len().to_string()),
        attribution.merges_analysed
    );
    Ok(())
}
