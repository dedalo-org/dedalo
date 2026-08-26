//! `dedalo propose` — what a round asks a human to sign.
//!
//! There is no `--execute` here on purpose. Dedalo holds no key and produces
//! no signature: it prints the exact instructions a multisig must run, with
//! the calldata already encoded, so a signer compares them against a plan they
//! can read rather than trusting a tool they cannot.

use anyhow::{Context, Result};

use crate::Engine;
use crate::chain::settlement::proposal::RoundProposal;

use crate::cli::args::ProposeArgs;
use crate::cli::commands::plan;
use crate::cli::ui::{self, Align, Table};

pub fn run(engine: &Engine, args: &ProposeArgs, json: bool) -> Result<()> {
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
            if args.save {
                engine.record_plan(&payout)?;
            }
            payout
        }
    };

    let proposal = RoundProposal::build(&payout, engine.config())?;

    if json {
        return crate::cli::commands::print_json(&proposal);
    }

    println!(
        "{} {} — {} {} across {} claim{}",
        ui::green("round"),
        payout.short_id(),
        payout.asset.format_amount(proposal.total),
        payout.asset.symbol,
        proposal.claims,
        if proposal.claims == 1 { "" } else { "s" }
    );
    println!("  {} {}", ui::dim("root"), proposal.merkle_root);
    println!();

    let mut table = Table::new(&[
        ("STEP", Align::Right),
        ("PROGRAM", Align::Left),
        ("WHAT", Align::Left),
    ]);
    for ix in &proposal.instructions {
        table.push(vec![
            ix.step.to_string(),
            ui::truncate(&ix.program_id, 20),
            ix.description.clone(),
        ]);
    }
    print!("{}", table.render());
    println!();

    // Accounts are printed, not summarised. Which accounts an instruction is
    // given decides what it does as much as its data does, and a signer who
    // checks only the data has checked half of it.
    for ix in &proposal.instructions {
        println!("{}", ui::dim(&format!("step {} accounts:", ix.step)));
        for account in &ix.accounts {
            let flags = match (account.signer, account.writable) {
                (true, true) => "signer, writable",
                (true, false) => "signer",
                (false, true) => "writable",
                (false, false) => "",
            };
            let what = account
                .address
                .clone()
                .or_else(|| account.derivation.clone().map(|d| format!("derived: {d}")))
                .unwrap_or_else(|| "unknown".into());
            println!(
                "  {:<44} {}  {}",
                what,
                ui::dim(&account.role),
                ui::dim(flags)
            );
        }
        println!(
            "{} {}",
            ui::dim(&format!("step {} data:", ix.step)),
            ix.data
        );
        println!();
    }
    println!(
        "{}",
        ui::dim(
            "run these in order from the project's multisig. Dedalo signed nothing: \
             check the data and the accounts against the plan before approving."
        )
    );
    Ok(())
}
