//! `dedalo scan` and `dedalo contributors` — read-only views of pending work.

use crate::Engine;
use anyhow::Result;

use crate::cli::args::RangeArgs;
use crate::cli::ui::{self, Align, Table};

pub fn scan(engine: &Engine, args: &RangeArgs, json: bool) -> Result<()> {
    let mut merges = engine.scan(args.since.as_deref())?;
    if let Some(limit) = args.limit {
        // Keep the most recent merges when truncating.
        let skip = merges.len().saturating_sub(limit);
        merges.drain(..skip);
    }

    if json {
        return crate::cli::commands::print_json(&merges);
    }

    if merges.is_empty() {
        println!(
            "No unpaid merges on {}.",
            ui::bold(&engine.config().git.branch)
        );
        return Ok(());
    }

    let policy = &engine.config().attribution;
    let mut table = Table::new(&[
        ("MERGE", Align::Left),
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
    println!(
        "{} merges pending on {}",
        ui::bold(&merges.len().to_string()),
        engine.config().git.branch
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
