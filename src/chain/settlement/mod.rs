//! Turning a verified plan into money that moved.
//!
//! Settlement is deliberately the thinnest layer in the system. Everything
//! that decides *who gets what* happens offline and deterministically in
//! [`crate::payout`]; a backend only signs and broadcasts what a plan already
//! says, after re-verifying it.

pub mod dry_run;
pub mod instruction;
pub mod proposal;
pub mod solana;

pub use dry_run::DryRunSettlement;
pub use solana::SolanaSettlement;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::config::SettlementConfig;
use crate::error::{Error, Result};
use crate::money::{Amount, Asset};
use crate::payout::PayoutPlan;

/// Proof that a plan was executed (or simulated).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettlementReceipt {
    /// Content hash of the plan that was executed.
    pub plan_id: String,
    /// Backend that executed it.
    pub backend: String,
    /// When it finished, as a unix timestamp.
    pub at: i64,
    /// Transaction hash, when a chain was actually touched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx: Option<String>,
    /// Total moved across all transfers.
    pub total: Amount,
    /// Number of non-zero transfers.
    pub transfers: usize,
    /// Whether this was a simulation rather than a broadcast.
    pub dry_run: bool,
}

/// A place money can be sent from.
#[async_trait]
pub trait Settlement: Send + Sync {
    /// Backend id, as written in `dedalo.toml` and the ledger.
    fn name(&self) -> &str;

    /// Whether this backend only simulates.
    fn is_dry_run(&self) -> bool {
        false
    }

    /// Spendable balance of the source wallet, if the backend can report it.
    async fn balance(&self, asset: &Asset) -> Result<Option<Amount>>;

    /// Execute the plan. Implementations must call [`PayoutPlan::verify`]
    /// before moving anything.
    async fn settle(&self, plan: &PayoutPlan) -> Result<SettlementReceipt>;
}

/// Build the backend named in the config.
pub fn backend_from_config(config: &SettlementConfig) -> Result<Box<dyn Settlement>> {
    match config.backend.as_str() {
        "dry-run" | "dryrun" | "simulate" => Ok(Box::new(DryRunSettlement::default())),
        "solana" => Ok(Box::new(SolanaSettlement::from_config(config)?)),
        other => Err(Error::config(format!(
            "unknown settlement backend `{other}` (expected `dry-run` or `solana`)"
        ))),
    }
}
