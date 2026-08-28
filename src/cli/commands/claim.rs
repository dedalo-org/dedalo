//! `dedalo claim` — what a contributor needs in order to be paid.
//!
//! A round is deposited against a Merkle root. To claim, a contributor needs
//! **their index, their amount, and their proof path**, and until now nothing
//! emitted any of them: `propose` gave the root and the instructions, and
//! `RoundProposal` counted the claims without ever handing one over. So the
//! pull model, as shipped, had no pull.
//!
//! # It reads a repository, not a chain
//!
//! Everything here comes from a plan in `.dedalo/objects` and the config beside
//! it. A contributor clones the repository and derives their own proof — they
//! do not ask the maintainer to send them a blob, and they do not need the
//! network. That is the same property the threat model claims for auditing, and
//! it would be strange for claiming to be the one thing you needed permission
//! for.
//!
//! # The proof is verified before it is printed
//!
//! `chain::merkle` can check a proof against the root it derived, so it does.
//! Printing an unverified proof would send somebody to a chain to find out.

use anyhow::{Context, Result};

use crate::Engine;
use crate::chain::merkle::ClaimTree;
use crate::payout::PayoutPlan;

use crate::cli::args::ClaimArgs;
use crate::cli::ui;

/// Everything a contributor hands to the claim program.
///
/// Serialised as-is by `--json`, so a front end or a script can build the
/// transaction without parsing a table.
#[derive(Debug, serde::Serialize)]
pub struct ClaimTicket {
    /// The round being claimed from. The program keys its replay guard on it.
    pub plan_id: String,
    /// Position in the tree. Derived from the plan's payable items, so it is
    /// stable only because their ordering is.
    pub index: u64,
    /// Who may claim, in the plan's canonical form.
    pub account: String,
    /// Handle the project knows this address by, for a human to check.
    pub handle: String,
    /// Base units, as a decimal string — never a JSON number.
    pub amount: crate::money::Amount,
    /// The same amount, rendered for a person.
    pub amount_display: String,
    /// Root this proof verifies against. Compare it with the deposit.
    pub merkle_root: String,
    /// Sibling hashes, leaf upward. Order matters.
    pub proof: Vec<String>,
    /// Program that holds the deposit, when the config names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claim_program: Option<String>,
}

/// Find the claimant's item, by wallet or by handle.
///
/// Wallet first, because a claimer knows their address and may not know what
/// handle the project gave them — but both are accepted, since the handle is
/// what they will see in a plan they read.
fn locate(plan: &PayoutPlan, needle: &str) -> Result<(u64, String, crate::money::Amount)> {
    let matches: Vec<(usize, &crate::payout::PayoutItem)> = plan
        .payable_items()
        .enumerate()
        .filter(|(_, item)| {
            item.wallet.as_str().eq_ignore_ascii_case(needle) || item.handle == needle
        })
        .collect();

    match matches.as_slice() {
        [] => {
            let known: Vec<&str> = plan
                .payable_items()
                .map(|item| item.handle.as_str())
                .collect();
            anyhow::bail!(
                "`{needle}` is not paid by plan {}\n  \
                 this round pays: {}\n  \
                 if you expected to be here, check `dedalo plan` for your address \
                 in the unresolved list",
                plan.short_id(),
                known.join(", ")
            )
        }
        [(index, item)] => Ok((*index as u64, item.handle.clone(), item.amount)),
        // Cannot happen while `PlanBuilder` merges one wallet into one item,
        // and worth refusing rather than guessing if it ever does: paying the
        // first match would be picking somebody's money by array order.
        many => anyhow::bail!(
            "`{needle}` matches {} items in plan {}, which should be impossible — \
             one wallet is merged into one transfer before a plan is finalised",
            many.len(),
            plan.short_id()
        ),
    }
}

pub fn run(engine: &Engine, args: &ClaimArgs, json: bool) -> Result<()> {
    let plan = engine
        .ledger()
        .load_plan(&args.plan)
        .with_context(|| format!("no saved plan `{}` in .dedalo/objects", args.plan))?;

    // A plan that does not hash to its own id is one somebody edited, and a
    // proof derived from it would verify against nothing.
    plan.verify()
        .context("the saved plan does not match its own id")?;

    let (index, handle, amount) = locate(&plan, &args.who)?;

    let tree = ClaimTree::from_plan(&plan)?;
    let proof = tree.proof(index as usize)?;
    let claim = &tree.claims()[index as usize];

    // Verified here, not on a chain. A wrong proof printed confidently is a
    // person spending gas to be told no.
    anyhow::ensure!(
        ClaimTree::verify(tree.root(), claim.leaf()?, &proof),
        "the proof for index {index} does not verify against the plan's own root — \
         this is a bug in dedalo, not in your configuration"
    );

    let ticket = ClaimTicket {
        plan_id: plan.id.clone(),
        index,
        account: claim.account.as_str().to_string(),
        handle,
        amount,
        amount_display: plan.asset.format_amount(amount),
        merkle_root: format!("0x{}", hex::encode(tree.root())),
        proof: proof
            .iter()
            .map(|hash| format!("0x{}", hex::encode(hash)))
            .collect(),
        claim_program: engine.config().settlement.program_id.clone(),
    };

    if json {
        return crate::cli::commands::print_json(&ticket);
    }

    println!("{}      {}", ui::dim("round"), plan.short_id());
    println!("{}      {}", ui::dim("index"), ticket.index);
    println!(
        "{}     {} {}",
        ui::dim("amount"),
        ui::bold(&ticket.amount_display),
        plan.asset.symbol
    );
    println!("{}    {}", ui::dim("account"), ticket.account);
    println!("{}       {}", ui::dim("root"), ticket.merkle_root);
    for (position, hash) in ticket.proof.iter().enumerate() {
        let label = if position == 0 { "proof" } else { "     " };
        println!("{}      {hash}", ui::dim(label));
    }
    if ticket.proof.is_empty() {
        println!(
            "{}      {}",
            ui::dim("proof"),
            ui::dim("(empty — this round has one payee, so the leaf is the root)")
        );
    }
    println!();

    match &ticket.claim_program {
        Some(program) => println!(
            "call claim(plan_id, index, account, amount, proof) on {}",
            ui::bold(program)
        ),
        None => println!(
            "{} no `settlement.program_id` in dedalo.toml, so this does not say \
             where to send it",
            ui::yellow("note:")
        ),
    }
    println!(
        "{}",
        ui::dim("nothing here is signed, and nothing has been sent")
    );

    Ok(())
}
