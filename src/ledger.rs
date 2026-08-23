//! The ledger: every round Dedalo recorded, as a hash chain.
//!
//! An append-only file is append-only by convention. Nothing stops someone
//! from editing a line, and nothing afterwards can tell. That is a weak
//! foundation for a record of who was paid what, so the ledger is a chain
//! instead: each entry names its parent, and an entry's id is a hash over its
//! parent's id and its own contents.
//!
//! ```text
//! HEAD ─▶ dedc9f… ──parent──▶ dedc41… ──parent──▶ dedc07… (root)
//!         settled            settled             plan-created
//! ```
//!
//! Change anything in an old entry and its id changes; every entry after it
//! named the old id, so their ids change too, and `HEAD` no longer matches.
//! One value therefore attests to the entire history, and
//! [`Ledger::verify`] is what checks it. Publish `HEAD` and anyone with a
//! clone can confirm that what they are reading is what was written.
//!
//! Entries live in the object store described in [`crate::store`], as plain
//! JSON, committed to the repository. A payout record that only exists on the
//! machine that made it is not a record anyone else can rely on — which is
//! also why this is not kept in `.git/`: a fresh clone has no `.git` state,
//! and a CI job that cannot see past rounds would happily pay them again.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::STATE_DIR;
use crate::error::{Error, Result};
use crate::money::Amount;
use crate::payout::{PayoutPlan, now_unix};
use crate::settlement::SettlementReceipt;
use crate::store::ObjectStore;

/// Id tag for a chain entry.
pub const ENTRY_TAG: &str = "dedc";
/// Id tag for a stored payout plan.
pub const PLAN_TAG: &str = "ded1";
/// Lock file taken for the duration of a settlement.
pub const LOCK_FILE: &str = "settle.lock";
/// The pre-chain event log. Present only in repositories written by an
/// earlier version; detected so its history cannot be silently ignored.
pub const LEGACY_LEDGER_FILE: &str = "ledger.jsonl";

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

    /// Feed this event into a hasher, field by field.
    ///
    /// Length-prefixed and in a fixed order, the same discipline
    /// [`PayoutPlan::compute_id`] uses and for the same reason: without a
    /// separator, two different events can produce one byte string. The
    /// leading discriminant means a `settled` entry can never hash like a
    /// `settlement-failed` one that happens to share its fields.
    ///
    /// Deliberately not `serde_json::to_string`: that would tie the id of
    /// every past entry to the declaration order of these structs, so
    /// reordering a field would silently rewrite history.
    fn absorb(&self, field: &mut impl FnMut(&[u8])) {
        match self {
            LedgerEntry::PlanCreated {
                at,
                plan_id,
                to_commit,
                merges,
                gross,
                payees,
            } => {
                field(b"plan-created");
                field(&at.to_be_bytes());
                field(plan_id.as_bytes());
                field(to_commit.as_bytes());
                field(&merges.to_be_bytes());
                field(&gross.base_units().to_be_bytes());
                field(&(*payees as u64).to_be_bytes());
            }
            LedgerEntry::Settled {
                at,
                plan_id,
                backend,
                tx,
                total,
                dry_run,
            } => {
                field(b"settled");
                field(&at.to_be_bytes());
                field(plan_id.as_bytes());
                field(backend.as_bytes());
                field(tx.as_deref().unwrap_or("").as_bytes());
                field(&total.base_units().to_be_bytes());
                field(&[u8::from(*dry_run)]);
            }
            LedgerEntry::SettlementFailed {
                at,
                plan_id,
                backend,
                reason,
            } => {
                field(b"settlement-failed");
                field(&at.to_be_bytes());
                field(plan_id.as_bytes());
                field(backend.as_bytes());
                field(reason.as_bytes());
            }
        }
    }
}

/// One link in the ledger chain: an event, and the entry before it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChainEntry {
    /// Id of the previous entry. `None` only for the very first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// What happened.
    pub event: LedgerEntry,
}

impl ChainEntry {
    /// Compute this entry's id from its parent and its event.
    ///
    /// The parent is part of the hash: that is the whole mechanism. An entry
    /// commits to the entire history behind it, so editing anything in the
    /// past changes every id since.
    pub fn id(&self) -> String {
        /// Bumped whenever the encoding below changes.
        const ENCODING_VERSION: u8 = 1;

        let mut hasher = Sha256::new();
        hasher.update(b"dedalo.ledger-entry.v");
        hasher.update([ENCODING_VERSION]);

        let mut field = |bytes: &[u8]| {
            hasher.update((bytes.len() as u64).to_be_bytes());
            hasher.update(bytes);
        };

        field(self.parent.as_deref().unwrap_or("").as_bytes());
        self.event.absorb(&mut field);

        format!("{ENTRY_TAG}{}", hex::encode(&hasher.finalize()[..16]))
    }
}

/// How far history has been paid out.
///
/// Derived by walking the chain rather than stored. A cached cursor is one
/// more thing that can disagree with the record it summarises, and when the
/// two disagree about `last_settled_commit`, the disagreement is a round paid
/// twice or a round skipped.
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
    store: ObjectStore,
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
            store: ObjectStore::at(root.as_ref().join(STATE_DIR)),
        })
    }

    /// The `.dedalo` directory this ledger lives in.
    pub fn dir(&self) -> &Path {
        self.store.dir()
    }

    /// The object store underneath.
    pub fn store(&self) -> &ObjectStore {
        &self.store
    }

    /// Id of the newest entry, or `None` when nothing has been recorded.
    pub fn head(&self) -> Result<Option<String>> {
        self.guard_legacy_layout()?;
        self.store.head()
    }

    /// Refuse to run against a ledger written before the chain existed.
    ///
    /// Reading it as empty would be the worst possible failure: `is_settled`
    /// would answer "no" for every past round, and a retried CI job would pay
    /// them all again. Better to stop and say so.
    fn guard_legacy_layout(&self) -> Result<()> {
        let legacy = self.store.dir().join(LEGACY_LEDGER_FILE);
        if legacy.is_file() && self.store.head()?.is_none() {
            return Err(Error::config(format!(
                "{} is from a version before the ledger became a hash chain, \
                 and this build cannot read it. Reading it as empty would let \
                 an already-paid round be paid again, so it is refused. Run \
                 `dedalo ledger migrate` to convert it.",
                legacy.display()
            )));
        }
        Ok(())
    }

    /// Append one event, linking it to the current head.
    ///
    /// Returns the new entry's id, which is also the new head.
    pub fn append(&self, event: &LedgerEntry) -> Result<String> {
        self.guard_legacy_layout()?;
        let entry = ChainEntry {
            parent: self.store.head()?,
            event: event.clone(),
        };
        let id = entry.id();
        self.store.write(&id, ENTRY_TAG, &entry)?;
        self.store.set_head(&id)?;
        Ok(id)
    }

    /// Read every event, oldest first.
    ///
    /// Walks from `HEAD` back to the root, so a stored object that nothing
    /// points at is not history and is not returned.
    pub fn entries(&self) -> Result<Vec<LedgerEntry>> {
        Ok(self.chain()?.into_iter().map(|(_, e)| e.event).collect())
    }

    /// The chain from oldest to newest, as `(id, entry)` pairs.
    pub fn chain(&self) -> Result<Vec<(String, ChainEntry)>> {
        self.guard_legacy_layout()?;
        let mut out = Vec::new();
        let mut cursor = self.store.head()?;
        // A cycle would otherwise spin forever on a hand-edited store. The
        // hash makes one practically impossible to construct, but "practically
        // impossible" is not a reason to write a loop that cannot terminate.
        let mut seen = std::collections::HashSet::new();
        while let Some(id) = cursor {
            if !seen.insert(id.clone()) {
                return Err(Error::LedgerCorrupt {
                    id: id.clone(),
                    reason: "entry is its own ancestor".into(),
                });
            }
            let entry: ChainEntry =
                self.store
                    .read(&id, ENTRY_TAG)
                    .map_err(|source| Error::LedgerCorrupt {
                        id: id.clone(),
                        reason: format!("entry is referenced but not readable: {source}"),
                    })?;
            cursor = entry.parent.clone();
            out.push((id, entry));
        }
        out.reverse();
        Ok(out)
    }

    /// Recompute every id and confirm the chain is intact.
    ///
    /// This is what makes the record worth trusting: an entry edited after
    /// the fact no longer hashes to the id that points at it, and every entry
    /// after it inherits the mismatch. Returns how many entries were checked.
    pub fn verify(&self) -> Result<usize> {
        let chain = self.chain()?;
        let mut expected_parent: Option<String> = None;
        for (id, entry) in &chain {
            if entry.parent != expected_parent {
                return Err(Error::LedgerCorrupt {
                    id: id.clone(),
                    reason: format!(
                        "parent is {:?}, but the entry before it is {:?}",
                        entry.parent, expected_parent
                    ),
                });
            }
            let recomputed = entry.id();
            if &recomputed != id {
                return Err(Error::LedgerCorrupt {
                    id: id.clone(),
                    reason: format!(
                        "contents hash to {recomputed}: it was changed after it was written"
                    ),
                });
            }
            expected_parent = Some(id.clone());
        }
        Ok(chain.len())
    }

    /// Derive the payout cursor by replaying the chain.
    pub fn state(&self) -> Result<State> {
        let mut state = State::default();
        for event in self.entries()? {
            if let LedgerEntry::Settled {
                at,
                plan_id,
                total,
                dry_run: false,
                ..
            } = event
            {
                state.last_settled_plan = Some(plan_id.clone());
                state.last_settled_at = Some(at);
                state.lifetime_paid = state.lifetime_paid.checked_add(total)?;
                // The commit and the protocol fee live on the plan, not the
                // event, so they are read back from the object it names.
                if let Ok(plan) = self.load_plan(&plan_id) {
                    state.last_settled_commit = Some(plan.range.to_commit.clone());
                    let protocol = plan
                        .items
                        .iter()
                        .filter(|item| item.kind == crate::payout::PayeeKind::Protocol)
                        .try_fold(Amount::ZERO, |acc, item| acc.checked_add(item.amount))?;
                    state.lifetime_protocol_fees =
                        state.lifetime_protocol_fees.checked_add(protocol)?;
                }
            }
        }
        Ok(state)
    }

    /// Where a given plan is stored.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] unless `plan_id` has the exact shape
    /// [`PayoutPlan::compute_id`] produces — see [`crate::store::validate_id`].
    pub fn plan_path(&self, plan_id: &str) -> Result<PathBuf> {
        self.store.path_of(plan_id, PLAN_TAG)
    }

    /// Persist the full plan so a settled round can always be reconstructed
    /// line by line, not just summarised.
    pub fn save_plan(&self, plan: &PayoutPlan) -> Result<PathBuf> {
        self.store.write(&plan.id, PLAN_TAG, plan)
    }

    /// Read a saved plan back, re-verifying it on the way in.
    ///
    /// The id is the content, so a plan whose file was edited no longer
    /// matches the name it is stored under. Both are checked.
    pub fn load_plan(&self, plan_id: &str) -> Result<PayoutPlan> {
        let plan: PayoutPlan = self.store.read(plan_id, PLAN_TAG)?;
        plan.verify()?;
        if plan.compute_id() != plan_id {
            return Err(Error::LedgerCorrupt {
                id: plan_id.to_string(),
                reason: "the stored plan does not hash to the id it is filed under".into(),
            });
        }
        Ok(plan)
    }

    /// Take the exclusive settlement lock for this ledger.
    ///
    /// Held for as long as the returned value lives. Dropping it releases the
    /// lock, including when settlement fails.
    ///
    /// # Errors
    ///
    /// Fails if another process already holds it.
    pub fn lock(&self) -> Result<SettlementLock> {
        SettlementLock::acquire(self.store.dir())
    }

    /// Has this exact plan already been settled? Guards against double payment
    /// when a CI job is retried.
    pub fn is_settled(&self, plan_id: &str) -> Result<bool> {
        Ok(self.entries()?.iter().any(|entry| {
            matches!(entry, LedgerEntry::Settled { dry_run: false, .. })
                && entry.plan_id() == plan_id
        }))
    }

    /// Convert a pre-chain `ledger.jsonl` into chain entries.
    ///
    /// Replays the file in order, so the resulting chain says exactly what the
    /// old log said and nothing more. The original is kept, renamed, because
    /// deleting the only copy of a payment record to upgrade a format is not
    /// a trade anyone should make silently.
    ///
    /// Returns how many entries were converted.
    ///
    /// # Errors
    ///
    /// Refuses if a chain already exists: replaying on top of one would
    /// duplicate every event, and duplicated settlements are the one thing
    /// this whole module exists to prevent.
    pub fn migrate_legacy(&self) -> Result<usize> {
        use std::io::{BufRead, BufReader};

        let legacy = self.store.dir().join(LEGACY_LEDGER_FILE);
        if !legacy.is_file() {
            return Ok(0);
        }
        if self.store.head()?.is_some() {
            return Err(Error::config(format!(
                "a chain already exists, so {} cannot be replayed onto it                  without recording every event twice",
                legacy.display()
            )));
        }

        let file = std::fs::File::open(&legacy).map_err(|e| Error::io(&legacy, e))?;
        let mut events = Vec::new();
        for (index, line) in BufReader::new(file).lines().enumerate() {
            let line = line.map_err(|e| Error::io(&legacy, e))?;
            if line.trim().is_empty() {
                continue;
            }
            // A corrupt line should name itself rather than fail anonymously.
            let event: LedgerEntry = serde_json::from_str(&line).map_err(|e| {
                Error::config(format!(
                    "{}:{}: cannot parse ledger entry: {e}",
                    legacy.display(),
                    index + 1
                ))
            })?;
            events.push(event);
        }

        // Renamed first: from here on `guard_legacy_layout` must not fire, or
        // the appends below would refuse the very file they are converting.
        let archived = legacy.with_extension("jsonl.migrated");
        std::fs::rename(&legacy, &archived).map_err(|e| Error::io(&legacy, e))?;

        for event in &events {
            self.append(event)?;
        }
        Ok(events.len())
    }

    /// Record a successful settlement and return the resulting cursor.
    pub fn record_settlement(
        &self,
        _plan: &PayoutPlan,
        receipt: &SettlementReceipt,
    ) -> Result<State> {
        self.append(&LedgerEntry::from_receipt(receipt))?;
        self.state()
    }
}
/// An exclusive claim on settling, released when dropped.
///
/// The ledger refuses to settle a plan id twice, but that check and the
/// settlement itself are two separate steps. Two `dedalo settle` runs racing
/// each other — two CI jobs on the same repository, a retried workflow
/// overlapping the original — can both read "not settled" before either
/// writes, and both broadcast.
///
/// Created with `create_new`, which is atomic: exactly one caller wins.
#[derive(Debug)]
pub struct SettlementLock {
    path: PathBuf,
}

impl SettlementLock {
    /// Take the lock, or fail because someone else holds it.
    fn acquire(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        let path = dir.join(LOCK_FILE);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                // Written for a human who finds a stale lock, not read back.
                let _ = writeln!(
                    file,
                    "pid {} since {}",
                    std::process::id(),
                    crate::payout::now_unix()
                );
                Ok(Self { path })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let holder = std::fs::read_to_string(&path).unwrap_or_default();
                Err(Error::config(format!(
                    "another settlement is in progress ({}). If that is wrong, \
                     delete {} and try again",
                    holder.trim(),
                    path.display()
                )))
            }
            Err(e) => Err(Error::io(&path, e)),
        }
    }
}

impl Drop for SettlementLock {
    fn drop(&mut self) {
        // A lock left behind is a nuisance the message above explains; failing
        // to clean up must not mask the error that got us here.
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "dedalo-ledger-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn settled(plan_id: &str, at: i64, dry_run: bool) -> LedgerEntry {
        LedgerEntry::Settled {
            at,
            plan_id: plan_id.into(),
            backend: "dry-run".into(),
            tx: None,
            total: Amount::from_base_units(1_000),
            dry_run,
        }
    }

    const PLAN_A: &str = "ded1aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const PLAN_B: &str = "ded1bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn only_one_settlement_can_hold_the_lock() {
        let root = temp_dir("lock");
        let ledger = Ledger::open(&root).unwrap();

        let first = ledger.lock().expect("the first caller takes it");
        let second = ledger.lock();
        assert!(second.is_err(), "a second caller must be refused");
        let message = second.unwrap_err().to_string();
        assert!(
            message.contains("another settlement is in progress"),
            "{message}"
        );
        assert!(
            message.contains(LOCK_FILE),
            "the message must name the file: {message}"
        );

        drop(first);
        assert!(ledger.lock().is_ok(), "the lock must be released on drop");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Reading must never write. A repository mounted read-only, or a checkout
    /// the user cannot modify, still has to answer `scan` and `contributors`.
    #[test]
    fn opening_a_ledger_creates_nothing() {
        let root = temp_dir("readonly");
        let ledger = Ledger::open(&root).unwrap();

        assert!(!ledger.dir().exists(), "open must not touch the filesystem");
        assert!(ledger.entries().unwrap().is_empty());
        assert_eq!(ledger.head().unwrap(), None);
        assert_eq!(ledger.state().unwrap(), State::default());
        assert!(!ledger.is_settled(PLAN_A).unwrap());
        assert_eq!(ledger.verify().unwrap(), 0);
        assert!(
            !ledger.dir().exists(),
            "reads must not create the directory"
        );

        ledger.append(&settled(PLAN_A, 0, true)).unwrap();
        assert!(ledger.dir().exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn entries_come_back_oldest_first_and_head_follows_the_newest() {
        let root = temp_dir("append");
        let ledger = Ledger::open(&root).unwrap();

        let first = ledger
            .append(&LedgerEntry::PlanCreated {
                at: 42,
                plan_id: PLAN_A.into(),
                to_commit: "deadbeef".into(),
                merges: 3,
                gross: Amount::from_base_units(1_000),
                payees: 4,
            })
            .unwrap();
        let second = ledger.append(&settled(PLAN_A, 43, false)).unwrap();

        assert_eq!(ledger.head().unwrap().as_deref(), Some(second.as_str()));
        let chain = ledger.chain().unwrap();
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].0, first);
        assert_eq!(chain[0].1.parent, None, "the root has no parent");
        assert_eq!(chain[1].1.parent.as_deref(), Some(first.as_str()));

        let events = ledger.entries().unwrap();
        assert_eq!(events[0].at(), 42, "oldest first");
        assert_eq!(events[1].at(), 43);
        assert_eq!(ledger.verify().unwrap(), 2);

        std::fs::remove_dir_all(&root).unwrap();
    }

    /// The point of the chain. An entry rewritten after the fact no longer
    /// hashes to the id that points at it, and `verify` says which one.
    #[test]
    fn editing_a_recorded_entry_is_detected() {
        let root = temp_dir("tamper");
        let ledger = Ledger::open(&root).unwrap();
        let first = ledger.append(&settled(PLAN_A, 100, false)).unwrap();
        ledger.append(&settled(PLAN_B, 200, false)).unwrap();
        assert_eq!(ledger.verify().unwrap(), 2);

        // Quietly move a payout from 1000 to 9000 in the first entry.
        let path = ledger.store().path_of(&first, ENTRY_TAG).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, raw.replace("\"1000\"", "\"9000\"")).unwrap();

        let error = ledger.verify().unwrap_err();
        let message = error.to_string();
        assert!(message.contains(&first), "must name the entry: {message}");
        assert!(
            message.contains("changed after it was written"),
            "must say what happened: {message}"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Deleting an entry does not shorten the history: the entry after it
    /// still names the one that is gone.
    #[test]
    fn deleting_a_recorded_entry_is_detected() {
        let root = temp_dir("delete");
        let ledger = Ledger::open(&root).unwrap();
        let first = ledger.append(&settled(PLAN_A, 100, false)).unwrap();
        ledger.append(&settled(PLAN_B, 200, false)).unwrap();

        std::fs::remove_file(ledger.store().path_of(&first, ENTRY_TAG).unwrap()).unwrap();

        let message = ledger.verify().unwrap_err().to_string();
        assert!(message.contains("referenced but not readable"), "{message}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn dry_runs_do_not_count_as_settled() {
        let root = temp_dir("dryrun");
        let ledger = Ledger::open(&root).unwrap();

        ledger.append(&settled(PLAN_A, 1, true)).unwrap();
        assert!(
            !ledger.is_settled(PLAN_A).unwrap(),
            "a simulation paid nobody"
        );
        assert_eq!(ledger.state().unwrap().lifetime_paid, Amount::ZERO);

        ledger.append(&settled(PLAN_A, 2, false)).unwrap();
        assert!(ledger.is_settled(PLAN_A).unwrap());
        assert_eq!(
            ledger.state().unwrap().lifetime_paid,
            Amount::from_base_units(1_000)
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// The cursor is derived, so it cannot drift from the record it summarises.
    #[test]
    fn the_cursor_is_replayed_from_the_chain() {
        let root = temp_dir("cursor");
        let ledger = Ledger::open(&root).unwrap();

        ledger.append(&settled(PLAN_A, 10, false)).unwrap();
        ledger.append(&settled(PLAN_B, 20, false)).unwrap();

        let state = ledger.state().unwrap();
        assert_eq!(state.last_settled_plan.as_deref(), Some(PLAN_B));
        assert_eq!(state.last_settled_at, Some(20));
        assert_eq!(state.lifetime_paid, Amount::from_base_units(2_000));

        // No cached copy exists to disagree with it.
        assert!(!root.join(STATE_DIR).join("state.json").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Reading an old ledger as empty would let every past round be paid
    /// again. Refusing is the only safe answer.
    #[test]
    fn a_pre_chain_ledger_is_refused_rather_than_read_as_empty() {
        let root = temp_dir("legacy");
        let dir = root.join(STATE_DIR);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(LEGACY_LEDGER_FILE),
            "{\"event\":\"settled\",\"at\":1,\"plan_id\":\"ded1old\",\
             \"backend\":\"evm\",\"total\":\"5000\",\"dry_run\":false}\n",
        )
        .unwrap();

        let ledger = Ledger::open(&root).unwrap();
        for result in [
            ledger.entries().map(|_| ()),
            ledger.is_settled(PLAN_A).map(|_| ()),
            ledger.head().map(|_| ()),
            ledger.append(&settled(PLAN_A, 1, false)).map(|_| ()),
        ] {
            let message = result.expect_err("must refuse").to_string();
            assert!(message.contains("hash chain"), "{message}");
            assert!(message.contains("paid again"), "{message}");
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// A change to a stored plan is caught by the id it is filed under, the
    /// same way an edited entry is caught by the chain.
    #[test]
    fn a_plan_edited_under_its_own_id_is_rejected() {
        use crate::attribution::Attribution;
        use crate::config::Config;
        use crate::payout::{PlanBuilder, PlanRange};

        let root = temp_dir("plan");
        let ledger = Ledger::open(&root).unwrap();
        let config = Config::template("dedalo");
        let plan = PlanBuilder::new(
            &config,
            &Attribution::default(),
            PlanRange {
                branch: "main".into(),
                from_commit: None,
                to_commit: "abc".into(),
                merges: 0,
            },
            Amount::from_base_units(1_000),
        )
        .created_at(0)
        .build()
        .unwrap();
        ledger.save_plan(&plan).unwrap();
        assert_eq!(ledger.load_plan(&plan.id).unwrap().id, plan.id);

        let path = ledger.plan_path(&plan.id).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, raw.replace(&plan.project, "somebody-else")).unwrap();

        // Editing the contents breaks the plan's own self-consistency, which
        // `verify` catches before the filename is even considered.
        let message = ledger.load_plan(&plan.id).unwrap_err().to_string();
        assert!(message.contains("plan id mismatch"), "{message}");

        // The other move: file an untouched, internally consistent plan under
        // a different plan's id, so a `settle --plan <id>` pays the wrong one.
        // Nothing inside the plan is wrong, so only the filename gives it away.
        std::fs::write(&path, raw).unwrap();
        let other = PLAN_B;
        let other_path = ledger.plan_path(other).unwrap();
        std::fs::create_dir_all(other_path.parent().unwrap()).unwrap();
        std::fs::copy(&path, &other_path).unwrap();

        let message = ledger.load_plan(other).unwrap_err().to_string();
        assert!(message.contains("does not hash to the id"), "{message}");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
