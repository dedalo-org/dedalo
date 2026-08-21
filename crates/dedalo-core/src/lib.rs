//! # Dedalo core
//!
//! Turns merges that are already in a git repository into a deterministic,
//! auditable payout plan, and settles that plan on chain.
//!
//! The pipeline has four stages, each usable on its own:
//!
//! 1. [`git`] reads merge commits from the repository — the only source of
//!    truth about who contributed what.
//! 2. [`attribution`] scores those merges into integer contribution weights.
//! 3. [`payout`] cuts fees ([`treasury`]), resolves contributors to wallets
//!    ([`identity`]) and produces a content-addressed [`payout::PayoutPlan`].
//! 4. [`settlement`] executes the plan, or simulates it.
//!
//! Everything before stage 4 is pure and offline: the same repository and the
//! same `dedalo.toml` always yield the same plan id, on any machine.
//!
//! ```no_run
//! use dedalo_core::{Engine, money::Amount};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let engine = Engine::discover(".")?;
//! let merges = engine.scan(None)?;
//! let attribution = engine.attribute(&merges);
//! let plan = engine.plan(&merges, &attribution, Amount::from_base_units(1_000_000))?;
//!
//! for item in plan.contributors() {
//!     println!("{:>12} {}", plan.asset.format_amount(item.amount), item.handle);
//! }
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]

pub mod attribution;
pub mod config;
/// Error and result types shared by every stage.
pub mod error;
pub mod git;
pub mod identity;
pub mod ledger;
pub mod money;
pub mod payout;
pub mod settlement;

#[cfg(feature = "testing")]
pub mod testing;

pub mod treasury;

pub use config::Config;
pub use error::{Error, Result};
pub use payout::PayoutPlan;

use std::path::{Path, PathBuf};

use attribution::Attribution;
use git::{CliGit, GitBackend, HistoryQuery, MergeEvent};
use ledger::{Ledger, LedgerEntry, State};
use money::Amount;
use payout::{PlanBuilder, PlanRange};
use settlement::{Settlement, SettlementReceipt};

/// Ties a repository, its config and its ledger together.
pub struct Engine {
    config: Config,
    config_path: PathBuf,
    repo: Box<dyn GitBackend>,
    ledger: Ledger,
}

impl Engine {
    /// Find `dedalo.toml` starting at `path` and open the repository around it.
    pub fn discover(path: impl AsRef<Path>) -> Result<Self> {
        let (config, config_path) = Config::discover(path.as_ref())?;
        let root = config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let repo = CliGit::discover(&root)?;
        let ledger = Ledger::open(repo.root())?;
        Ok(Self {
            config,
            config_path,
            repo: Box::new(repo),
            ledger,
        })
    }

    /// Assemble an engine from parts, for tests or alternative git backends.
    pub fn new(
        config: Config,
        config_path: PathBuf,
        repo: Box<dyn GitBackend>,
        ledger: Ledger,
    ) -> Self {
        Self {
            config,
            config_path,
            repo,
            ledger,
        }
    }

    /// The loaded funding policy.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Where that policy was loaded from.
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// Mutable access, for commands that edit the policy.
    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    /// The repository this engine reads history from.
    pub fn repo(&self) -> &dyn GitBackend {
        self.repo.as_ref()
    }

    /// The append-only record of plans and settlements.
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// The payout cursor: how far history has been paid out.
    pub fn state(&self) -> Result<State> {
        self.ledger.state()
    }

    /// Merges on the configured branch that have not been paid out yet.
    ///
    /// `since` overrides the ledger cursor; passing `None` continues from the
    /// last settled commit, so re-running never pays for the same work twice.
    pub fn scan(&self, since: Option<&str>) -> Result<Vec<MergeEvent>> {
        let cursor = match since {
            Some(rev) => Some(self.repo.resolve(rev)?),
            None => self.state()?.last_settled_commit,
        };
        let query = HistoryQuery {
            branch: self.config.git.branch.clone(),
            since_commit: cursor,
            since_timestamp: None,
            limit: None,
        };
        let merges = self.repo.merges(&query)?;
        Ok(merges
            .into_iter()
            .filter(|merge| !self.config.is_ignored_subject(&merge.subject))
            .collect())
    }

    /// Score merges under the project's attribution policy.
    pub fn attribute(&self, merges: &[MergeEvent]) -> Attribution {
        Attribution::compute(merges, &self.config.attribution)
    }

    /// Build a plan distributing `gross` over the given merges.
    pub fn plan(
        &self,
        merges: &[MergeEvent],
        attribution: &Attribution,
        gross: Amount,
    ) -> Result<PayoutPlan> {
        let head = self.repo.resolve(&self.config.git.branch)?;
        let range = PlanRange {
            branch: self.config.git.branch.clone(),
            from_commit: self.state()?.last_settled_commit,
            // Anchor to the newest merge, not to HEAD: unmerged commits after
            // the last merge are not part of this round.
            to_commit: merges.last().map(|m| m.sha.clone()).unwrap_or(head),
            merges: merges.len() as u64,
        };
        PlanBuilder::new(&self.config, attribution, range, gross).build()
    }

    /// Persist a plan and record that it was created.
    pub fn record_plan(&self, plan: &PayoutPlan) -> Result<PathBuf> {
        let path = self.ledger.save_plan(plan)?;
        self.ledger.append(&LedgerEntry::from_plan(plan))?;
        Ok(path)
    }

    /// Settle a plan through `backend`, refusing to pay the same plan twice.
    pub async fn settle(
        &self,
        plan: &PayoutPlan,
        backend: &dyn Settlement,
    ) -> Result<SettlementReceipt> {
        plan.verify()?;
        if !backend.is_dry_run() && self.ledger.is_settled(&plan.id)? {
            return Err(Error::Settlement {
                backend: backend.name().to_string(),
                reason: format!("plan {} was already settled", plan.short_id()),
            });
        }

        // Refuse to start a round the source wallet cannot cover.
        if let Some(balance) = backend.balance(&self.config.asset).await? {
            let total = plan.total()?;
            if balance < total {
                return Err(Error::Settlement {
                    backend: backend.name().to_string(),
                    reason: format!(
                        "source wallet holds {} {} but the round needs {}",
                        self.config.asset.format_amount(balance),
                        self.config.asset.symbol,
                        self.config.asset.format_amount(total)
                    ),
                });
            }
        }

        match backend.settle(plan).await {
            Ok(receipt) => {
                self.ledger.record_settlement(plan, &receipt)?;
                Ok(receipt)
            }
            Err(error) => {
                // A failed round is still history worth keeping.
                self.ledger.append(&LedgerEntry::SettlementFailed {
                    at: payout::now_unix(),
                    plan_id: plan.id.clone(),
                    backend: backend.name().to_string(),
                    reason: error.to_string(),
                })?;
                Err(error)
            }
        }
    }
}
