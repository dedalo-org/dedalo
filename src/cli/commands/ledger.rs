//! `dedalo ledger` — the append-only history of what Dedalo did.

use crate::Engine;
use crate::ledger::LedgerEntry;
use anyhow::Result;

use crate::cli::args::LedgerArgs;
use crate::cli::ui::{self, Align, Table};

pub fn run(engine: &Engine, args: &LedgerArgs, json: bool) -> Result<()> {
    let mut entries = engine.ledger().entries()?;
    let skip = entries.len().saturating_sub(args.limit);
    entries.drain(..skip);

    if json {
        return crate::cli::commands::print_json(&entries);
    }

    if entries.is_empty() {
        println!("{}", ui::dim("no ledger entries yet"));
        return Ok(());
    }

    let asset = &engine.config().asset;
    let mut table = Table::new(&[
        ("DATE", Align::Left),
        ("EVENT", Align::Left),
        ("PLAN", Align::Left),
        ("DETAIL", Align::Left),
    ]);
    for entry in &entries {
        let (event, detail) = match entry {
            LedgerEntry::PlanCreated {
                merges,
                gross,
                payees,
                ..
            } => (
                "plan".to_string(),
                format!(
                    "{} {} over {merges} merges → {payees} payees",
                    asset.format_amount(*gross),
                    asset.symbol
                ),
            ),
            LedgerEntry::Settled {
                backend,
                tx,
                total,
                dry_run,
                ..
            } => (
                if *dry_run {
                    ui::dim("simulated")
                } else {
                    ui::green("settled")
                },
                format!(
                    "{} {} via {backend}{}",
                    asset.format_amount(*total),
                    asset.symbol,
                    tx.as_deref()
                        .map(|t| format!(" tx {}", ui::truncate(t, 12)))
                        .unwrap_or_default()
                ),
            ),
            LedgerEntry::SettlementFailed {
                backend, reason, ..
            } => (
                ui::yellow("failed"),
                format!("{backend}: {}", ui::truncate(reason, 60)),
            ),
        };
        table.push(vec![
            ui::format_timestamp(entry.at()),
            event,
            entry.plan_id()[..entry.plan_id().len().min(12)].to_string(),
            detail,
        ]);
    }
    print!("{}", table.render());
    Ok(())
}
