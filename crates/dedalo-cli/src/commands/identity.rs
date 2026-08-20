//! `dedalo identity` — the git-email to wallet mapping.
//!
//! Edits go through `toml_edit` so the comments a maintainer wrote in
//! `dedalo.toml` survive an automated `identity link`.

use std::path::Path;

use anyhow::{Context, Result, bail};
use dedalo_core::Engine;
use toml_edit::{Array, DocumentMut, Item, Table, value};

use crate::cli::{IdentityCommand, RangeArgs};
use crate::commands::display_path;
use crate::ui::{self, Align, Table as OutTable};

pub fn run(engine: &Engine, command: &IdentityCommand, json: bool) -> Result<()> {
    match command {
        IdentityCommand::List => list(engine, json),
        IdentityCommand::Link {
            handle,
            wallet,
            emails,
        } => link(engine, handle, wallet, emails, json),
        IdentityCommand::Remove { handle } => remove(engine, handle, json),
        IdentityCommand::Missing(range) => missing(engine, range, json),
    }
}

fn list(engine: &Engine, json: bool) -> Result<()> {
    let identities = &engine.config().identities;
    if json {
        return crate::commands::print_json(identities);
    }
    if identities.is_empty() {
        println!(
            "{}",
            ui::dim(
                "no identities yet — run `dedalo identity link <handle> <wallet> --email <email>`"
            )
        );
        return Ok(());
    }
    let mut table = OutTable::new(&[
        ("HANDLE", Align::Left),
        ("WALLET", Align::Left),
        ("EMAILS", Align::Left),
        ("", Align::Left),
    ]);
    for identity in identities {
        table.push(vec![
            identity.handle.clone(),
            ui::truncate(&identity.wallet, 20),
            identity.emails.join(", "),
            if identity.excluded {
                ui::dim("excluded")
            } else {
                String::new()
            },
        ]);
    }
    print!("{}", table.render());
    Ok(())
}

fn link(engine: &Engine, handle: &str, wallet: &str, emails: &[String], json: bool) -> Result<()> {
    if wallet.trim().is_empty() {
        bail!("a wallet address is required");
    }
    let path = engine.config_path();
    let mut doc = load_document(path)?;
    let tables = identities_array(&mut doc);

    let existing = tables
        .iter_mut()
        .find(|table| table.get("handle").and_then(Item::as_str) == Some(handle));

    match existing {
        Some(table) => {
            table["wallet"] = value(wallet);
            let mut list = table
                .get("emails")
                .and_then(Item::as_array)
                .cloned()
                .unwrap_or_default();
            for email in emails {
                let email = email.trim().to_ascii_lowercase();
                let already = list
                    .iter()
                    .filter_map(|v| v.as_str())
                    .any(|v| v.eq_ignore_ascii_case(&email));
                if !already {
                    list.push(email);
                }
            }
            table["emails"] = value(list);
        }
        None => {
            let mut table = Table::new();
            table["handle"] = value(handle);
            table["wallet"] = value(wallet);
            let mut list = Array::new();
            for email in emails {
                list.push(email.trim().to_ascii_lowercase());
            }
            table["emails"] = value(list);
            tables.push(table);
        }
    }

    write_document(path, &doc)?;

    if json {
        return crate::commands::print_json(&serde_json::json!({
            "handle": handle,
            "wallet": wallet,
            "emails": emails,
        }));
    }
    println!(
        "{} {} → {} ({})",
        ui::green("linked"),
        handle,
        ui::truncate(wallet, 20),
        emails.join(", ")
    );
    Ok(())
}

fn remove(engine: &Engine, handle: &str, json: bool) -> Result<()> {
    let path = engine.config_path();
    let mut doc = load_document(path)?;
    let tables = identities_array(&mut doc);
    let before = tables.len();
    tables.retain(|table| table.get("handle").and_then(Item::as_str) != Some(handle));
    if tables.len() == before {
        bail!("no identity with handle `{handle}`");
    }
    write_document(path, &doc)?;

    if json {
        return crate::commands::print_json(&serde_json::json!({ "removed": handle }));
    }
    println!("{} {handle}", ui::green("removed"));
    Ok(())
}

fn missing(engine: &Engine, range: &RangeArgs, json: bool) -> Result<()> {
    let merges = engine.scan(range.since.as_deref())?;
    let attribution = engine.attribute(&merges);
    let identities = engine.config().identity_map();

    let unlinked: Vec<_> = attribution
        .contributions
        .iter()
        .filter(|c| {
            identities.resolve(&c.author).is_none()
                && !engine.config().is_ignored_email(&c.author.email)
        })
        .collect();

    if json {
        return crate::commands::print_json(&unlinked);
    }
    if unlinked.is_empty() {
        println!("{}", ui::green("every pending contributor has a wallet"));
        return Ok(());
    }

    let mut table = OutTable::new(&[
        ("NAME", Align::Left),
        ("EMAIL", Align::Left),
        ("SCORE", Align::Right),
        ("SHARE", Align::Right),
    ]);
    for contribution in &unlinked {
        table.push(vec![
            ui::truncate(&contribution.author.name, 24),
            contribution.author.email.clone(),
            ui::format_score(contribution.score),
            ui::format_bps(attribution.share_bps(contribution)),
        ]);
    }
    print!("{}", table.render());
    println!();
    println!(
        "{}",
        ui::dim("link them with: dedalo identity link <handle> <wallet> --email <email>")
    );
    Ok(())
}

fn load_document(path: &Path) -> Result<DocumentMut> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read {}", display_path(path)))?;
    raw.parse::<DocumentMut>()
        .with_context(|| format!("cannot parse {}", display_path(path)))
}

fn write_document(path: &Path, doc: &DocumentMut) -> Result<()> {
    std::fs::write(path, doc.to_string())
        .with_context(|| format!("cannot write {}", display_path(path)))
}

/// Get the `[[identities]]` array, creating it if the file has none yet.
fn identities_array(doc: &mut DocumentMut) -> &mut toml_edit::ArrayOfTables {
    if !doc.contains_key("identities") || doc["identities"].as_array_of_tables().is_none() {
        doc["identities"] = Item::ArrayOfTables(toml_edit::ArrayOfTables::new());
    }
    doc["identities"]
        .as_array_of_tables_mut()
        .expect("just ensured it is an array of tables")
}
