//! `dedalo` — merge-to-earn funding for open source.

mod cli;
mod commands;
mod ui;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    if let Err(error) = run(&cli).await {
        eprintln!("{} {error:#}", ui::yellow("error:"));
        std::process::exit(1);
    }
}

async fn run(cli: &Cli) -> Result<()> {
    // `init` is the one command that runs before a config exists.
    if let Command::Init(args) = &cli.command {
        return commands::init::run(args, cli.repo.as_ref(), cli.json);
    }

    let engine = commands::engine(cli.repo.as_ref())?;
    match &cli.command {
        Command::Init(_) => unreachable!("handled above"),
        Command::Scan(args) => commands::scan::scan(&engine, args, cli.json),
        Command::Contributors(args) => commands::scan::contributors(&engine, args, cli.json),
        Command::Plan(args) => commands::plan::run(&engine, args, cli.json),
        Command::Settle(args) => commands::settle::run(&engine, args, cli.json).await,
        Command::Status => commands::status::run(&engine, cli.json),
        Command::Identity(command) => commands::identity::run(&engine, command, cli.json),
        Command::Ledger(args) => commands::ledger::run(&engine, args, cli.json),
    }
}

fn init_tracing(verbosity: u8) {
    use tracing_subscriber::EnvFilter;

    let default = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_env("DEDALO_LOG").unwrap_or_else(|_| EnvFilter::new(default));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .without_time()
        .with_writer(std::io::stderr)
        .init();
}
