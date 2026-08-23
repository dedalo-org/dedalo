//! `dedalo settle` — execute a round, or simulate it.
//!
//! Simulation is the default. Real money only moves behind `--execute`, and
//! only through the backend configured in `dedalo.toml`.

use crate::settlement::{DryRunSettlement, backend_from_config};
use crate::{Engine, SettlementOptions};
use anyhow::{Context, Result};

use crate::cli::args::SettleArgs;
use crate::cli::commands::plan;
use crate::cli::ui;

pub async fn run(engine: &Engine, args: &SettleArgs, json: bool) -> Result<()> {
    let payout = match &args.plan {
        Some(id) => engine
            .ledger()
            .load_plan(id)
            .with_context(|| format!("no saved plan `{id}`"))?,
        None => {
            let amount = args
                .amount
                .as_deref()
                .expect("clap guarantees --amount when --plan is absent");
            let (payout, _) = plan::build(engine, amount, &args.range)?;
            // A round that is about to be paid must exist on disk first, so a
            // crash mid-broadcast still leaves the exact numbers behind.
            engine.record_plan(&payout)?;
            payout
        }
    };

    let backend = if args.execute {
        backend_from_config(&engine.config().settlement)?
    } else {
        Box::new(DryRunSettlement::default())
    };

    if !json {
        plan::print_plan(engine, &payout);
        println!();
    }

    let options = if args.allow_undistributed {
        SettlementOptions::allowing_undistributed()
    } else {
        SettlementOptions::strict()
    };
    let receipt = engine
        .settle_with(&payout, backend.as_ref(), &options)
        .await?;

    if json {
        return crate::cli::commands::print_json(&receipt);
    }

    if receipt.dry_run {
        println!(
            "{} {} transfers totalling {} {} — nothing was sent",
            ui::yellow("simulated:"),
            receipt.transfers,
            payout.asset.format_amount(receipt.total),
            payout.asset.symbol
        );
        println!(
            "  {}",
            ui::dim("re-run with --execute to settle through the configured backend")
        );
    } else {
        println!(
            "{} {} transfers totalling {} {} via {}",
            ui::green("settled:"),
            receipt.transfers,
            payout.asset.format_amount(receipt.total),
            payout.asset.symbol,
            receipt.backend
        );
        if let Some(tx) = &receipt.tx {
            println!("  tx {tx}");
        }
    }
    Ok(())
}
