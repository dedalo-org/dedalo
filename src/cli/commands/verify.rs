//! `dedalo verify` — check that the ledger says what it was written to say.
//!
//! The whole point of the chain is that this command can be run by someone
//! who does not trust whoever produced the record. It reads only what is
//! committed to the repository, touches no network, and needs no key.

use anyhow::{Context, Result};

use crate::Engine;
use crate::ledger::LedgerEntry;

use crate::cli::ui;

pub fn run(engine: &Engine, json: bool) -> Result<()> {
    let ledger = engine.ledger();

    let entries = ledger
        .verify()
        .context("the ledger chain does not hold together")?;

    // A chain that hashes correctly can still point at a plan that is missing
    // or was swapped. The entries say what was paid; the plans say to whom.
    let mut plans_checked = 0usize;
    let mut missing = Vec::new();
    for event in ledger.entries()? {
        if let LedgerEntry::Settled {
            plan_id,
            dry_run: false,
            ..
        } = &event
        {
            match ledger.load_plan(plan_id) {
                Ok(_) => plans_checked += 1,
                Err(error) => missing.push((plan_id.clone(), error.to_string())),
            }
        }
    }

    let head = ledger.head()?;

    if json {
        return crate::cli::commands::print_json(&serde_json::json!({
            "ok": missing.is_empty(),
            "head": head,
            "entries": entries,
            "plans_checked": plans_checked,
            "problems": missing
                .iter()
                .map(|(id, reason)| serde_json::json!({ "plan": id, "reason": reason }))
                .collect::<Vec<_>>(),
        }));
    }

    match &head {
        Some(head) => println!("{} {}", ui::dim("head"), head),
        None => {
            println!("{}", ui::dim("no ledger entries yet — nothing to verify"));
            return Ok(());
        }
    }
    if entries == 1 {
        println!("{} 1 entry hashes to its recorded id", ui::green("ok"));
    } else {
        println!(
            "{} {entries} entries hash to their recorded ids",
            ui::green("ok")
        );
    }

    if missing.is_empty() {
        println!(
            "{} {plans_checked} settled plan{} present and self-consistent",
            ui::green("ok"),
            if plans_checked == 1 { "" } else { "s" }
        );
        println!();
        println!(
            "{}",
            ui::dim("recompute a round from the same history and config to check the amounts too")
        );
        return Ok(());
    }

    for (plan, reason) in &missing {
        eprintln!("{} plan {plan}: {reason}", ui::yellow("problem:"));
    }
    anyhow::bail!(
        "{} of {} settled plans could not be verified",
        missing.len(),
        missing.len() + plans_checked
    )
}
