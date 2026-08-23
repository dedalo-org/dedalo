//! `dedalo status` — where the project's funding stands right now.

use crate::Engine;
use anyhow::Result;
use serde_json::json;

use crate::cli::commands::display_path;
use crate::cli::ui;

pub fn run(engine: &Engine, json: bool) -> Result<()> {
    let config = engine.config();
    let state = engine.state()?;
    let merges = engine.scan(None)?;
    let attribution = engine.attribute(&merges);
    let asset = &config.asset;

    if json {
        return crate::cli::commands::print_json(&json!({
            "project": config.project.name,
            "branch": config.git.branch,
            "asset": asset,
            "pending_merges": merges.len(),
            "pending_contributors": attribution.contributions.len(),
            "fees": {
                "protocol_bps": config.fees.protocol_bps,
                "treasury_bps": config.fees.treasury_bps,
                "contributor_bps": config.fees.contributor_bps(),
            },
            "settlement_backend": config.settlement.backend,
            "state": state,
        }));
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
