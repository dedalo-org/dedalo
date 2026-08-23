//! `dedalo propose` — what a round asks a human to sign.
//!
//! There is no `--execute` here on purpose. Dedalo holds no key and produces
//! no signature: it prints the exact transactions a multisig must run, with
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
        ("TO", Align::Left),
        ("WHAT", Align::Left),
    ]);
    for tx in &proposal.transactions {
        table.push(vec![
            tx.step.to_string(),
            ui::truncate(&tx.to, 20),
            tx.description.clone(),
        ]);
    }
    print!("{}", table.render());
    println!();

    for tx in &proposal.transactions {
        println!("{} {}", ui::dim(&format!("calldata {}:", tx.step)), tx.data);
    }
    println!();
    println!(
        "{}",
        ui::dim(
            "run these in order from the project's multisig. Dedalo signed nothing: \
             check the calldata against the plan before approving."
        )
    );
    Ok(())
}
