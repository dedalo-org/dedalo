//! Command line surface.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "dedalo",
    version,
    about = "Turn code merges into sustainable open-source funding",
    long_about = "Dedalo reads merge history from git, scores contributions, and \
                  distributes a funding round to contributor wallets — taking a \
                  protocol fee that flows to the network's Open Collective."
)]
/// One parsed `dedalo` invocation: the global options, plus the command.
///
/// Build it with `Cli::parse()` and hand it to [`crate::cli::run`].
pub struct Cli {
    /// Repository to operate on. Defaults to the current directory.
    #[arg(long, short = 'C', global = true, value_name = "PATH")]
    pub repo: Option<PathBuf>,

    /// Emit machine-readable JSON instead of tables.
    #[arg(long, global = true)]
    pub json: bool,

    /// Increase log verbosity (repeatable).
    #[arg(long, short = 'v', global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
/// Everything `dedalo` can be asked to do.
///
/// The doc comment on each variant is what `--help` prints, so it is written
/// for someone at a terminal rather than for a reader of the API reference.
pub enum Command {
    /// Create a dedalo.toml in this repository.
    Init(InitArgs),

    /// List merges that have not been paid out yet.
    Scan(RangeArgs),

    /// Show contribution scores for the pending range.
    Contributors(RangeArgs),

    /// Compute a payout plan for a funding round.
    Plan(PlanArgs),

    /// Execute a payout plan. Simulates unless --execute is given.
    Settle(SettleArgs),

    /// Show the current funding state of the project.
    Status,

    /// Manage the git-identity to wallet mapping.
    #[command(subcommand)]
    Identity(IdentityCommand),

    /// Print the append-only event ledger.
    Ledger(LedgerArgs),
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Project name. Defaults to the repository directory name.
    #[arg(long)]
    pub name: Option<String>,

    /// Open Collective slug that receives the protocol fee.
    #[arg(long, value_name = "SLUG")]
    pub open_collective: Option<String>,

    /// Overwrite an existing dedalo.toml.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args, Clone)]
pub struct RangeArgs {
    /// Start after this revision instead of the last settled commit.
    #[arg(long, value_name = "REV")]
    pub since: Option<String>,

    /// Show at most this many entries.
    #[arg(long)]
    pub limit: Option<usize>,
}

#[derive(Debug, Args)]
pub struct PlanArgs {
    /// Size of the round, as a decimal amount of the configured asset.
    #[arg(long, value_name = "AMOUNT")]
    pub amount: String,

    #[command(flatten)]
    pub range: RangeArgs,

    /// Write the plan to .dedalo/plans and record it in the ledger.
    #[arg(long)]
    pub save: bool,
}

#[derive(Debug, Args)]
pub struct SettleArgs {
    /// Settle a plan that was already saved, by id.
    #[arg(long, value_name = "PLAN_ID", conflicts_with = "amount")]
    pub plan: Option<String>,

    /// Compute a fresh plan of this size and settle it.
    #[arg(long, value_name = "AMOUNT", required_unless_present = "plan")]
    pub amount: Option<String>,

    #[command(flatten)]
    pub range: RangeArgs,

    /// Actually broadcast, using the backend from dedalo.toml.
    #[arg(long)]
    pub execute: bool,

    /// Settle even though part of the contributor pool has no destination.
    ///
    /// Only meaningful when nobody in the round has a wallet on file, which
    /// normally means a `dedalo identity link` is missing rather than that
    /// you meant to send the fees alone.
    #[arg(long)]
    pub allow_undistributed: bool,
}

#[derive(Debug, Subcommand)]
pub enum IdentityCommand {
    /// List known identities.
    List,

    /// Map a git email to a wallet.
    Link {
        /// Handle used in reports, e.g. a GitHub username.
        handle: String,
        /// Destination wallet address.
        wallet: String,
        /// Git author email to attach. Repeatable.
        #[arg(long = "email", value_name = "EMAIL", required = true)]
        emails: Vec<String>,
    },

    /// Remove an identity by handle.
    Remove { handle: String },

    /// Show contributors in history that have no wallet yet.
    Missing(RangeArgs),
}

#[derive(Debug, Args)]
pub struct LedgerArgs {
    /// Show only the last N entries.
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
}
