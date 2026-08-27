//! `dedalo status` — where the project's funding stands right now.

use crate::Engine;
use anyhow::Result;

use crate::cli::commands::display_path;
use crate::cli::ui;
use crate::money::Asset;
use crate::money::treasury::FeeSchedule;
use crate::storage::ledger::State;
use serde::Serialize;

/// What `dedalo status --json` emits.
///
/// A type rather than a `serde_json::json!` literal, and the difference is not
/// tidiness. `action.yml` parses this and [RELEASING.md] makes renaming a field
/// a breaking change — so the shape is a contract, and a contract assembled
/// from a macro is one nobody can read without running it. Every field below
/// is documented, `tests/cli.rs` pins the ones the Action reads, and the
/// handbook's `--json` reference is written from this rather than from memory.
///
/// Terminal output is not API and this is: the two travel together and only
/// one of them may change freely.
///
/// [RELEASING.md]: https://github.com/dedalo-org/dedalo/blob/main/RELEASING.md
#[derive(Debug, Clone, Serialize)]
pub struct StatusReport<'a> {
    /// Project name, from `[project] name`.
    pub project: &'a str,
    /// Branch whose landed changes earn a payout.
    pub branch: &'a str,
    /// The token contributors are paid in.
    pub asset: &'a Asset,
    /// How a landed change is recognised on this branch.
    pub lands_as: crate::git::LandsAs,
    /// Changes that have landed since the last settled round.
    pub pending_changes: usize,
    /// How many distinct contributors those changes are attributed to.
    pub pending_contributors: usize,
    /// The cut taken before contributors are paid.
    pub fees: FeeReport,
    /// Backend a round would settle through.
    pub settlement_backend: &'a str,
    /// How many identities the config maps.
    pub identities: usize,
    /// What the ledger records, or `None` where there is no ledger yet.
    pub state: &'a State,
}

/// The fee schedule, with the contributor share spelled out.
///
/// `contributor_bps` is derived rather than configured — it is whatever the
/// other two leave. Emitted anyway, because a consumer that computed it would
/// be a second implementation of the one subtraction that decides how much
/// contributors get.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct FeeReport {
    /// Share routed to the network.
    pub protocol_bps: u16,
    /// Share retained by the project.
    pub treasury_bps: u16,
    /// What is left for contributors.
    pub contributor_bps: u16,
}

impl From<&FeeSchedule> for FeeReport {
    fn from(fees: &FeeSchedule) -> Self {
        Self {
            protocol_bps: fees.protocol_bps,
            treasury_bps: fees.treasury_bps,
            contributor_bps: fees.contributor_bps(),
        }
    }
}

pub fn run(engine: &Engine, json: bool) -> Result<()> {
    let config = engine.config();
    let state = engine.state()?;
    let merges = engine.scan(None)?;
    let attribution = engine.attribute(&merges);
    let asset = &config.asset;

    if json {
        return crate::cli::commands::print_json(&StatusReport {
            project: &config.project.name,
            branch: &config.git.branch,
            asset,
            lands_as: config.git.lands_as,
            pending_changes: merges.len(),
            pending_contributors: attribution.contributions.len(),
            fees: FeeReport::from(&config.fees),
            settlement_backend: &config.settlement.backend,
            identities: config.identities.len(),
            state: &state,
        });
    }

    println!(
        "{}  {}",
        ui::bold(&config.project.name),
        ui::dim(&display_path(engine.config_path()))
    );
    println!();
    println!("  branch            {}", config.git.branch);
    println!("  asset             {} on {}", asset.symbol, asset.chain);
    println!(
        "  split             {} contributors / {} treasury / {} protocol",
        ui::format_bps(config.fees.contributor_bps() as u32),
        ui::format_bps(config.fees.treasury_bps as u32),
        ui::format_bps(config.fees.protocol_bps as u32),
    );
    println!("  backend           {}", config.settlement.backend);
    println!("  identities        {}", config.identities.len());
    println!();
    println!(
        "  pending           {} merges, {} contributors",
        ui::bold(&merges.len().to_string()),
        attribution.contributions.len()
    );
    match &state.last_settled_at {
        Some(at) => println!(
            "  last round        {} ({})",
            state.last_settled_plan.as_deref().unwrap_or("-"),
            ui::format_timestamp(*at)
        ),
        None => println!("  last round        {}", ui::dim("never settled")),
    }
    println!(
        "  paid to date      {} {}",
        asset.format_amount(state.lifetime_paid),
        asset.symbol
    );
    println!(
        "  protocol fees     {} {}  {}",
        asset.format_amount(state.lifetime_protocol_fees),
        asset.symbol,
        ui::dim(&format!(
            "→ {}",
            config
                .project
                .open_collective
                .as_deref()
                .unwrap_or("open collective")
        ))
    );

    let unlinked = attribution
        .contributions
        .iter()
        .filter(|c| config.identity_map().resolve(&c.author).is_none())
        .count();
    if unlinked > 0 {
        println!();
        println!(
            "{} {unlinked} pending contributor(s) have no wallet — see `dedalo identity missing`",
            ui::yellow("warning:")
        );
    }
    Ok(())
}
