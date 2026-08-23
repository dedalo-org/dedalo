//! `dedalo plan` — price a funding round without spending anything.

use crate::money::Amount;
use crate::payout::{PayeeKind, PayoutPlan, UnresolvedReason};
use crate::{Engine, git::MergeEvent};
use anyhow::{Context, Result};

use crate::cli::args::{PlanArgs, RangeArgs};
use crate::cli::commands::display_path;
use crate::cli::ui::{self, Align, Table};

/// Build a plan from the pending range. Shared with `dedalo settle`.
pub fn build(
    engine: &Engine,
    amount: &str,
    range: &RangeArgs,
) -> Result<(PayoutPlan, Vec<MergeEvent>)> {
    let asset = &engine.config().asset;
    let gross = asset
        .parse_amount(amount)
        .with_context(|| format!("`{amount}` is not a valid {} amount", asset.symbol))?;
    if gross == Amount::ZERO {
        anyhow::bail!("the round amount must be greater than zero");
    }

    let merges = engine.scan(range.since.as_deref())?;
    let attribution = engine.attribute(&merges);
    let plan = engine.plan(&merges, &attribution, gross)?;
    Ok((plan, merges))
}

pub fn run(engine: &Engine, args: &PlanArgs, json: bool) -> Result<()> {
    let (plan, _merges) = build(engine, &args.amount, &args.range)?;

    if args.save {
        let path = engine.record_plan(&plan)?;
        if !json {
            println!("{} {}", ui::green("saved"), display_path(&path));
            println!();
        }
    }

    if json {
        return crate::cli::commands::print_json(&plan);
    }

    print_plan(engine, &plan);
    Ok(())
}

/// The human-readable rendering of a plan, reused by `settle`.
pub fn print_plan(engine: &Engine, plan: &PayoutPlan) {
    let asset = &plan.asset;
    println!(
        "{} {}  {}",
        ui::bold("Round"),
        plan.short_id(),
        ui::dim(&format!(
            "{} merges on {} → {}",
            plan.range.merges,
            plan.range.branch,
            &plan.range.to_commit[..plan.range.to_commit.len().min(8)]
        ))
    );
    println!(
        "{} {} {}",
        ui::bold("Gross"),
        asset.format_amount(plan.split.gross),
        asset.symbol
    );
    println!();

    let mut table = Table::new(&[
        ("PAYEE", Align::Left),
        ("KIND", Align::Left),
        ("WALLET", Align::Left),
        ("SHARE", Align::Right),
        ("AMOUNT", Align::Right),
    ]);
    for item in &plan.items {
        let kind = match item.kind {
            PayeeKind::Contributor => ui::dim("contributor"),
            PayeeKind::Treasury => ui::dim("treasury"),
            PayeeKind::Protocol => ui::green("protocol"),
        };
        table.push(vec![
            ui::truncate(&item.handle, 24),
            kind,
            ui::truncate(item.wallet.as_str(), 14),
            ui::format_bps(item.share_bps),
            asset.format_amount(item.amount),
        ]);
    }
    print!("{}", table.render());

    println!();
    let contributors_total: u128 = plan
        .contributors()
        .map(|item| item.amount.base_units())
        .sum();
    println!(
        "  contributors  {:>16} {}",
        asset.format_amount(crate::money::Amount::from_base_units(contributors_total)),
        asset.symbol
    );
    println!(
        "  treasury      {:>16} {}",
        asset.format_amount(plan.split.treasury),
        asset.symbol
    );
    println!(
        "  protocol fee  {:>16} {}  {}",
        asset.format_amount(plan.split.protocol),
        asset.symbol,
        ui::dim(&format!(
            "→ {}",
            engine
                .config()
                .project
                .open_collective
                .as_deref()
                .unwrap_or("open collective")
        ))
    );

    if !plan.undistributed.is_zero() {
        println!();
        println!(
            "{} {} {} of the contributor pool has no destination and will stay in the",
            ui::yellow("undistributed:"),
            asset.format_amount(plan.undistributed),
            asset.symbol
        );
        println!(
            "  source wallet. {}",
            ui::dim("every contributor in this round is missing a wallet")
        );
    }

    if !plan.unresolved.is_empty() {
        println!();
        let unpaid: Vec<_> = plan
            .unresolved
            .iter()
            .filter(|u| u.reason == UnresolvedReason::NoWallet)
            .collect();
        if !unpaid.is_empty() {
            println!(
                "{} {} contributor(s) earned a share but have no wallet:",
                ui::yellow("warning:"),
                unpaid.len()
            );
            for entry in unpaid.iter().take(5) {
                println!(
                    "  {} <{}>  {} points",
                    entry.name,
                    entry.email,
                    ui::format_score(entry.score)
                );
            }
            // Only true when somebody was left to receive it.
            let fate = if plan.undistributed.is_zero() {
                "their share went to the contributors who do have one"
            } else {
                "nobody in this round has a wallet, so their share went nowhere"
            };
            println!("  {}", ui::dim(fate));
            println!(
                "  {}",
                ui::dim("link them with `dedalo identity link <handle> <wallet> --email <email>`")
            );
        }
    }
}
