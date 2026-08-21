//! The local ledger: an append-only record of everything Dedalo did.
//!
//! Entries are newline-delimited JSON so they diff cleanly, can be committed
//! to the repo, and can be replayed by anyone verifying a round. The ledger
//! never stores balances — it stores events, and balances are derived.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::STATE_DIR;
use crate::error::{Error, Result};
use crate::money::Amount;
use crate::payout::{PayoutPlan, now_unix};
use crate::settlement::SettlementReceipt;

/// Newline-delimited JSON file holding every recorded event.
pub const LEDGER_FILE: &str = "ledger.jsonl";
/// JSON file holding the payout cursor.
pub const STATE_FILE: &str = "state.json";

/// One recorded event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum LedgerEntry {
    /// A round was computed and frozen.
    PlanCreated {
        /// When the plan was frozen, as a unix timestamp.
        at: i64,
        /// Content hash of the plan.
        plan_id: String,
        /// Newest merge included in the round.
        to_commit: String,
        /// Merges the round covers.
        merges: u64,
        /// Size of the round before fees.
        gross: Amount,
        /// Number of transfers the plan contains.
        payees: usize,
    },
    /// A round was submitted to a settlement backend.
    Settled {
        /// When settlement returned, as a unix timestamp.
        at: i64,
        /// Content hash of the plan that was executed.
        plan_id: String,
        /// Backend that executed it.
        backend: String,
        /// Transaction hash, when a chain was actually touched.
        #[serde(skip_serializing_if = "Option::is_none")]
        tx: Option<String>,
        /// Total moved.
        total: Amount,
        /// Whether this was a simulation.
        dry_run: bool,
    },
    /// A round could not be settled. Kept so retries are auditable.
    SettlementFailed {
        /// When the attempt failed, as a unix timestamp.
        at: i64,
        /// Content hash of the plan that was not executed.
        plan_id: String,
        /// Backend that refused or errored.
        backend: String,
        /// Why it failed, as reported to the user.
        reason: String,
    },
}

impl LedgerEntry {
    /// When this event happened.
    pub fn at(&self) -> i64 {
        match self {
            LedgerEntry::PlanCreated { at, .. }
            | LedgerEntry::Settled { at, .. }
            | LedgerEntry::SettlementFailed { at, .. } => *at,
        }
    }

    /// The plan this event is about.
    pub fn plan_id(&self) -> &str {
        match self {
            LedgerEntry::PlanCreated { plan_id, .. }
            | LedgerEntry::Settled { plan_id, .. }
            | LedgerEntry::SettlementFailed { plan_id, .. } => plan_id,
        }
    }

    /// Summarise a freshly built plan as an event.
    pub fn from_plan(plan: &PayoutPlan) -> Self {
        LedgerEntry::PlanCreated {
            at: now_unix(),
            plan_id: plan.id.clone(),
            to_commit: plan.range.to_commit.clone(),
            merges: plan.range.merges,
            gross: plan.split.gross,
            payees: plan.items.len(),
        }
    }

    /// Turn a settlement receipt into an event.
    pub fn from_receipt(receipt: &SettlementReceipt) -> Self {
        LedgerEntry::Settled {
            at: receipt.at,
            plan_id: receipt.plan_id.clone(),
            backend: receipt.backend.clone(),
            tx: receipt.tx.clone(),
            total: receipt.total,
            dry_run: receipt.dry_run,
        }
    }
}

/// Cursor tracking how far history has been paid out.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// Last commit included in a settled round. The next round starts here.
    pub last_settled_commit: Option<String>,
    /// Content hash of the last plan that was really executed.
    pub last_settled_plan: Option<String>,
    /// When that happened, as a unix timestamp.
    pub last_settled_at: Option<i64>,
    /// Total ever paid out, in base units, for quick status output.
    pub lifetime_paid: Amount,
    /// Total protocol fees this project has contributed to the network.
    pub lifetime_protocol_fees: Amount,
}

/// Reads and writes `.dedalo/`.
#[derive(Debug, Clone)]
pub struct Ledger {
    dir: PathBuf,
}

impl Ledger {
    /// Point at the `.dedalo` directory under `root`, without creating it.
    ///
    /// Creating it here would mean every read touched the disk: `dedalo scan`
    /// on a read-only checkout, or on a repository mounted `:ro` into a
    /// container, would fail before reading anything. The directory is created
    /// on the first write instead.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            dir: root.as_ref().join(STATE_DIR),
        })
    }

    /// Create the state directory. Called by every write path, never by a read.
    fn ensure_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.dir).map_err(|e| Error::io(&self.dir, e))
    }

    /// The `.dedalo` directory this ledger lives in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Path of the event log.
    pub fn ledger_path(&self) -> PathBuf {
        self.dir.join(LEDGER_FILE)
    }

    /// Path of the cursor file.
    pub fn state_path(&self) -> PathBuf {
        self.dir.join(STATE_FILE)
    }

    /// Where a given plan is stored.
    pub fn plan_path(&self, plan_id: &str) -> PathBuf {
        self.dir.join("plans").join(format!("{plan_id}.json"))
    }

    /// Append one event. The log is never rewritten.
    pub fn append(&self, entry: &LedgerEntry) -> Result<()> {
        self.ensure_dir()?;
        let path = self.ledger_path();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| Error::io(&path, e))?;
        let line = serde_json::to_string(entry)?;
        writeln!(file, "{line}").map_err(|e| Error::io(&path, e))
    }

    /// Read every event, oldest first.
    pub fn entries(&self) -> Result<Vec<LedgerEntry>> {
        let path = self.ledger_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = File::open(&path).map_err(|e| Error::io(&path, e))?;
        let mut entries = Vec::new();
        for (index, line) in BufReader::new(file).lines().enumerate() {
            let line = line.map_err(|e| Error::io(&path, e))?;
            if line.trim().is_empty() {
                continue;
            }
            // A corrupt line should name itself rather than fail anonymously.
            let entry: LedgerEntry = serde_json::from_str(&line).map_err(|e| {
                Error::config(format!(
                    "{}:{}: cannot parse ledger entry: {e}",
                    path.display(),
                    index + 1
                ))
            })?;
            entries.push(entry);
        }
        Ok(entries)
    }

    /// Read the payout cursor, defaulting to "never settled".
    pub fn state(&self) -> Result<State> {
        let path = self.state_path();
        if !path.exists() {
            return Ok(State::default());
        }
        let raw = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        Ok(serde_json::from_str(&raw)?)
    }

    /// Overwrite the payout cursor.
    pub fn save_state(&self, state: &State) -> Result<()> {
        self.ensure_dir()?;
        let path = self.state_path();
        let raw = serde_json::to_string_pretty(state)?;
        std::fs::write(&path, raw).map_err(|e| Error::io(&path, e))
    }

    /// Persist the full plan next to the ledger so a settled round can always
    /// be reconstructed line by line, not just summarised.
    pub fn save_plan(&self, plan: &PayoutPlan) -> Result<PathBuf> {
        let path = self.plan_path(&plan.id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let raw = serde_json::to_string_pretty(plan)?;
        std::fs::write(&path, raw).map_err(|e| Error::io(&path, e))?;
        Ok(path)
    }

    /// Read a saved plan back, re-verifying it on the way in.
    pub fn load_plan(&self, plan_id: &str) -> Result<PayoutPlan> {
        let path = self.plan_path(plan_id);
        let raw = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        let plan: PayoutPlan = serde_json::from_str(&raw)?;
        plan.verify()?;
        Ok(plan)
    }

    /// Has this exact plan already been settled? Guards against double payment
    /// when a CI job is retried.
    pub fn is_settled(&self, plan_id: &str) -> Result<bool> {
        Ok(self.entries()?.iter().any(|entry| {
            matches!(entry, LedgerEntry::Settled { dry_run: false, .. })
                && entry.plan_id() == plan_id
        }))
    }

    /// Record a successful settlement and advance the cursor.
    pub fn record_settlement(
        &self,
        plan: &PayoutPlan,
        receipt: &SettlementReceipt,
    ) -> Result<State> {
        self.append(&LedgerEntry::from_receipt(receipt))?;
        let mut state = self.state()?;
        if !receipt.dry_run {
            state.last_settled_commit = Some(plan.range.to_commit.clone());
            state.last_settled_plan = Some(plan.id.clone());
            state.last_settled_at = Some(receipt.at);
            state.lifetime_paid = state.lifetime_paid.checked_add(receipt.total)?;
            let protocol = plan
                .items
                .iter()
                .filter(|item| item.kind == crate::payout::PayeeKind::Protocol)
                .try_fold(Amount::ZERO, |acc, item| acc.checked_add(item.amount))?;
            state.lifetime_protocol_fees = state.lifetime_protocol_fees.checked_add(protocol)?;
            self.save_state(&state)?;
        }
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dedalo-ledger-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Reading must never write. A repository mounted read-only, or a checkout
    /// the user cannot modify, still has to answer `scan` and `contributors`.
    #[test]
    fn opening_a_ledger_creates_nothing() {
        let root = temp_dir("readonly");
        let ledger = Ledger::open(&root).unwrap();

        assert!(!ledger.dir().exists(), "open must not touch the filesystem");
        assert!(ledger.entries().unwrap().is_empty());
        assert_eq!(ledger.state().unwrap(), State::default());
        assert!(!ledger.is_settled("ded1abc").unwrap());
        assert!(
            !ledger.dir().exists(),
            "reads must not create the directory"
        );

        // The first write is what brings it into existence.
        ledger
            .append(&LedgerEntry::SettlementFailed {
                at: 0,
                plan_id: "ded1abc".into(),
                backend: "dry-run".into(),
                reason: "test".into(),
            })
            .unwrap();
        assert!(ledger.dir().exists());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn appends_and_reads_back_entries() {
        let root = temp_dir("append");
        let ledger = Ledger::open(&root).unwrap();
        assert!(ledger.entries().unwrap().is_empty());

        let entry = LedgerEntry::PlanCreated {
            at: 42,
            plan_id: "ded1abc".into(),
            to_commit: "deadbeef".into(),
            merges: 3,
            gross: Amount::from_base_units(1_000),
            payees: 4,
        };
        ledger.append(&entry).unwrap();
        ledger.append(&entry).unwrap();

        let entries = ledger.entries().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], entry);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn dry_runs_do_not_count_as_settled() {
        let root = temp_dir("dryrun");
        let ledger = Ledger::open(&root).unwrap();
        ledger
            .append(&LedgerEntry::Settled {
                at: 1,
                plan_id: "ded1abc".into(),
                backend: "dry-run".into(),
                tx: None,
                total: Amount::from_base_units(10),
                dry_run: true,
            })
            .unwrap();
        assert!(!ledger.is_settled("ded1abc").unwrap());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
