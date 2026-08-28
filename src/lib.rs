//! # Dedalo
//!
//! Turns merges that are already in a git repository into a deterministic,
//! auditable payout plan, and settles that plan on chain.
//!
//! The pipeline has four stages, each usable on its own:
//!
//! 1. [`git`] reads merge commits from the repository — the only source of
//!    truth about who contributed what.
//! 2. [`attribution`] scores those merges into integer contribution weights.
//! 3. [`payout`] cuts fees ([`money::treasury`]), resolves contributors to wallets
//!    ([`attribution::identity`]) and produces a content-addressed [`payout::PayoutPlan`].
//! 4. [`chain::settlement`] executes the plan, or simulates it.
//!
//! Everything before stage 4 is pure and offline: the same repository and the
//! same `dedalo.toml` always yield the same plan id, on any machine.
//!
//! ```no_run
//! use dedalo::{Engine, money::Amount};
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
// docs.rs and CI's rustdoc job both build with `--cfg docsrs` on nightly. It
// exists so that a feature-gated item says which feature gates it, rather than
// appearing in the reference as if it were always there — `dedalo::testing` in
// particular reads like part of the API until you try to use it without
// turning the feature on. Nothing else in the crate reads this cfg, and a
// stable build ignores it entirely.
#![cfg_attr(docsrs, feature(doc_cfg))]

// One module per concern, and every one of them a directory. A file at the
// top of `src/` is a concern nobody has decided the shape of yet.
//
// Each module documents itself, in its own `//!` header. A `///` here as well
// would merge into that header from a different scope, and every intra-doc
// link inside it would resolve against the crate root instead of the module.

pub mod attribution;
pub mod chain;
pub mod git;
pub mod lifecycle;
pub mod money;
pub mod payout;
pub mod storage;

pub mod config;
pub mod error;

#[cfg(feature = "cli")]
#[cfg_attr(docsrs, doc(cfg(feature = "cli")))]
pub mod cli;

#[cfg(feature = "testing")]
#[cfg_attr(docsrs, doc(cfg(feature = "testing")))]
pub mod testing;

pub use config::Config;
pub use error::{Error, Result};
pub use payout::PayoutPlan;

use std::path::{Path, PathBuf};

use attribution::Attribution;
use chain::settlement::{Settlement, SettlementReceipt};
use git::{CliGit, GitBackend, HistoryQuery, MergeEvent};
use money::Amount;
use payout::{PlanBuilder, PlanRange};
use storage::ledger::{Ledger, LedgerEntry, State};

/// What a settlement is allowed to do.
///
/// Every field here switches off a refusal, so the default — [`SettlementOptions::strict`]
/// — is the one that says no most often. Loosening it should take a
/// deliberate flag and a moment's thought.
#[derive(Debug, Clone, Default)]
pub struct SettlementOptions {
    /// Allow settling a round whose contributor pool reached nobody.
    ///
    /// Only ever true when the operator has looked at
    /// [`PayoutPlan::undistributed`] and decided the fees alone are what they
    /// meant to send.
    pub allow_undistributed: bool,
}

impl SettlementOptions {
    /// Refuse everything refusable. The default.
    pub fn strict() -> Self {
        Self::default()
    }

    /// Permit a round that distributes nothing to contributors.
    pub fn allowing_undistributed() -> Self {
        Self {
            allow_undistributed: true,
        }
    }
}

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

    /// Resolve the configured branch to a ref that exists in this checkout.
    ///
    /// CI checkouts are detached: `actions/checkout` fetches the commit under
    /// test without creating a local branch, so `main` does not resolve even
    /// though `origin/main` does. Falling back to the remote-tracking ref is
    /// what lets the same config work on a laptop and in a pipeline.
    ///
    /// The returned ref is only ever used to query git. A plan always records
    /// the branch *name* from the config, never the ref it resolved to, or the
    /// same history would produce two different plan ids.
    fn resolve_branch(&self) -> Result<String> {
        let branch = &self.config.git.branch;
        if self.repo.resolve(branch).is_ok() {
            return Ok(branch.clone());
        }
        let remote = format!("origin/{branch}");
        if self.repo.resolve(&remote).is_ok() {
            tracing::debug!("`{branch}` is not a local ref; using `{remote}`");
            return Ok(remote);
        }
        Err(Error::config(format!(
            "branch `{branch}` does not exist here, and neither does `{remote}`. \
             In CI, check out with `fetch-depth: 0` so the branch is fetched."
        )))
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
            branch: self.resolve_branch()?,
            since_commit: cursor,
            since_timestamp: None,
            limit: None,
            lands_as: self.config.git.lands_as,
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
        let head = self.repo.resolve(&self.resolve_branch()?)?;
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
    ///
    /// Equivalent to [`Engine::settle_with`] under [`SettlementOptions::strict`],
    /// which is the only default a payments tool should have.
    pub async fn settle(
        &self,
        plan: &PayoutPlan,
        backend: &dyn Settlement,
    ) -> Result<SettlementReceipt> {
        self.settle_with(plan, backend, &SettlementOptions::strict())
            .await
    }

    /// Settle a plan, with the refusals spelled out.
    ///
    /// Every check here answers the same question: is there any reading of
    /// this plan under which money goes somewhere it cannot come back from?
    pub async fn settle_with(
        &self,
        plan: &PayoutPlan,
        backend: &dyn Settlement,
        options: &SettlementOptions,
    ) -> Result<SettlementReceipt> {
        plan.verify()?;

        let refuse = |reason: String| Error::Settlement {
            backend: backend.name().to_string(),
            reason,
        };

        // Held across the whole of settlement, so the "already settled?" check
        // below and the record written afterwards cannot be split by a second
        // process reading between them. A simulation moves nothing and does
        // not need to exclude anyone.
        let _lock = if backend.is_dry_run() {
            None
        } else {
            Some(self.ledger.lock()?)
        };

        if !backend.is_dry_run() && self.ledger.is_settled(&plan.id)? {
            return Err(refuse(format!(
                "plan {} was already settled",
                plan.short_id()
            )));
        }

        // The zero address is a valid encoding that nobody holds the key to.
        // It is the placeholder `dedalo init` writes, so a config that was
        // never finished reaches here looking perfectly well-formed.
        for item in plan.payable_items() {
            if item.wallet.is_zero() {
                return Err(refuse(format!(
                    "`{}` is set to the zero address, which destroys anything sent to it; \
                     set a real address in dedalo.toml",
                    item.handle
                )));
            }
        }

        // A round where nobody could be paid sends only fees. That is almost
        // always a missing `dedalo identity link`, not an intention.
        if !plan.undistributed.is_zero() && !options.allow_undistributed {
            return Err(refuse(format!(
                "{} of the contributor pool has no destination, because {} contributor(s) \
                 earned a share with no wallet on file. Link them, or settle anyway if \
                 that is really what you mean",
                self.config.asset.format_amount(plan.undistributed),
                plan.unresolved.len()
            )));
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
