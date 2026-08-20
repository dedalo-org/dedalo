//! Payout plans: the auditable artifact between git history and a transaction.
//!
//! A plan is pure data. It is computed offline, can be reviewed in a pull
//! request, and is content-addressed by a hash of everything that determines
//! it — range, policy, fees, and the resulting line items. Settling a plan
//! elsewhere with a different id means someone changed the numbers.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::attribution::Attribution;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::money::{Amount, Asset};
use crate::treasury::TreasurySplit;

/// Why a given address is receiving money.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PayeeKind {
    /// A contributor, paid by attribution weight.
    Contributor,
    /// The project's own reserve.
    Treasury,
    /// The network's Open Collective wallet.
    Protocol,
}

impl PayeeKind {
    /// Stable string used in the plan hash and in output.
    pub fn label(&self) -> &'static str {
        match self {
            PayeeKind::Contributor => "contributor",
            PayeeKind::Treasury => "treasury",
            PayeeKind::Protocol => "protocol",
        }
    }
}

/// One transfer in a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayoutItem {
    /// Why this address is being paid.
    pub kind: PayeeKind,
    /// Handle for contributors, or a fixed label for the fee recipients.
    pub handle: String,
    /// Destination address.
    pub wallet: String,
    /// Exactly what this address receives.
    pub amount: Amount,
    /// Attribution weight in milli-points; zero for fee recipients.
    #[serde(default)]
    pub score: u128,
    /// Share of the gross round in basis points, for human review.
    #[serde(default)]
    pub share_bps: u32,
}

/// The commit range a plan covers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanRange {
    /// Branch the merges were read from.
    pub branch: String,
    /// Exclusive lower bound: the last commit already paid for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_commit: Option<String>,
    /// Newest merge included in the round.
    pub to_commit: String,
    /// How many merges the round covers.
    pub merges: u64,
}

/// A complete, settleable round.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayoutPlan {
    /// Content hash of the plan, prefixed `ded1`.
    pub id: String,
    /// Project the round belongs to.
    pub project: String,
    /// When the plan was built. Deliberately not part of [`PayoutPlan::id`].
    pub created_at: i64,
    /// Token every amount is denominated in.
    pub asset: Asset,
    /// Commit range the round pays for.
    pub range: PlanRange,
    /// How the gross amount was cut before distribution.
    pub split: TreasurySplit,
    /// Every transfer, contributors first.
    pub items: Vec<PayoutItem>,
    /// Contributors that earned a share but have no wallet on file. Their
    /// weight is excluded from the split, never silently redistributed
    /// without being reported.
    #[serde(default)]
    pub unresolved: Vec<UnresolvedContributor>,
}

/// Someone who earned a share but could not be paid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnresolvedContributor {
    /// Name as written in their commits.
    pub name: String,
    /// Email that could not be mapped to a wallet.
    pub email: String,
    /// Weight they would have been paid on.
    pub score: u128,
    /// Why they were left out.
    pub reason: UnresolvedReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// Why a contributor was excluded from a plan's transfers.
#[serde(rename_all = "kebab-case")]
pub enum UnresolvedReason {
    /// No identity in `dedalo.toml` maps this email to a wallet.
    NoWallet,
    /// Explicitly excluded (bot, or already compensated).
    Excluded,
    /// Listed under `git.ignore_emails`.
    Ignored,
}

impl PayoutPlan {
    /// Total that will actually leave the source wallet.
    pub fn total(&self) -> Result<Amount> {
        self.items
            .iter()
            .try_fold(Amount::ZERO, |acc, item| acc.checked_add(item.amount))
    }

    /// Just the contributor transfers, without the fee recipients.
    pub fn contributors(&self) -> impl Iterator<Item = &PayoutItem> {
        self.items
            .iter()
            .filter(|item| item.kind == PayeeKind::Contributor)
    }

    /// Items worth broadcasting: zero-value transfers only burn gas.
    pub fn payable_items(&self) -> impl Iterator<Item = &PayoutItem> {
        self.items.iter().filter(|item| !item.amount.is_zero())
    }

    /// Re-check every invariant. Settlement backends call this before signing.
    pub fn verify(&self) -> Result<()> {
        if !self.split.is_balanced() {
            return Err(Error::config("plan split does not sum to the gross amount"));
        }
        let total = self.total()?;
        if total > self.split.gross {
            return Err(Error::config(format!(
                "plan pays out {total} base units but the round is only {}",
                self.split.gross
            )));
        }
        let recomputed = self.compute_id();
        if recomputed != self.id {
            return Err(Error::config(format!(
                "plan id mismatch: expected {recomputed}, found {}",
                self.id
            )));
        }
        Ok(())
    }

    /// Deterministic content hash over everything that fixes the outcome.
    pub fn compute_id(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.project.as_bytes());
        hasher.update([0]);
        hasher.update(self.asset.symbol.as_bytes());
        hasher.update(self.asset.chain.as_bytes());
        hasher.update(self.range.branch.as_bytes());
        hasher.update(self.range.from_commit.as_deref().unwrap_or("").as_bytes());
        hasher.update(self.range.to_commit.as_bytes());
        hasher.update(self.split.gross.base_units().to_be_bytes());
        hasher.update(self.split.protocol.base_units().to_be_bytes());
        hasher.update(self.split.treasury.base_units().to_be_bytes());
        for item in &self.items {
            hasher.update([0]);
            hasher.update(item.kind.label().as_bytes());
            hasher.update(item.wallet.as_bytes());
            hasher.update(item.amount.base_units().to_be_bytes());
        }
        format!("ded1{}", hex::encode(&hasher.finalize()[..16]))
    }

    /// Abbreviated plan id, for terminal output.
    pub fn short_id(&self) -> &str {
        &self.id[..self.id.len().min(12)]
    }
}

/// Assembles a [`PayoutPlan`] from attribution plus config.
pub struct PlanBuilder<'a> {
    config: &'a Config,
    attribution: &'a Attribution,
    range: PlanRange,
    gross: Amount,
    created_at: i64,
}

impl<'a> PlanBuilder<'a> {
    /// Prepare a builder for a round of `gross` over `range`.
    pub fn new(
        config: &'a Config,
        attribution: &'a Attribution,
        range: PlanRange,
        gross: Amount,
    ) -> Self {
        Self {
            config,
            attribution,
            range,
            gross,
            created_at: now_unix(),
        }
    }

    /// Pin the creation timestamp, so tests and CI reruns stay reproducible.
    pub fn created_at(mut self, timestamp: i64) -> Self {
        self.created_at = timestamp;
        self
    }

    /// Produce a verified, content-addressed plan.
    pub fn build(self) -> Result<PayoutPlan> {
        let split = self.config.fees.split(self.gross)?;
        let identities = self.config.identity_map();

        // Partition contributors into payable and unresolved, keeping the
        // attribution order so the split stays deterministic.
        let mut payable = Vec::new();
        let mut unresolved = Vec::new();
        for contribution in &self.attribution.contributions {
            let author = &contribution.author;
            if self.config.is_ignored_email(&author.email) {
                unresolved.push(UnresolvedContributor {
                    name: author.name.clone(),
                    email: author.email.clone(),
                    score: contribution.score,
                    reason: UnresolvedReason::Ignored,
                });
                continue;
            }
            match identities.resolve(author) {
                Some(identity) if !identity.excluded && !identity.wallet.trim().is_empty() => {
                    payable.push((
                        identity.handle.clone(),
                        identity.wallet.clone(),
                        contribution,
                    ));
                }
                Some(identity) => unresolved.push(UnresolvedContributor {
                    name: author.name.clone(),
                    email: author.email.clone(),
                    score: contribution.score,
                    reason: if identity.excluded {
                        UnresolvedReason::Excluded
                    } else {
                        UnresolvedReason::NoWallet
                    },
                }),
                None => unresolved.push(UnresolvedContributor {
                    name: author.name.clone(),
                    email: author.email.clone(),
                    score: contribution.score,
                    reason: UnresolvedReason::NoWallet,
                }),
            }
        }

        // One wallet may back several handles; merging keeps a single transfer
        // per address so the batch cannot double-send.
        let weights: Vec<u128> = payable.iter().map(|(_, _, c)| c.score).collect();
        let shares = split.contributors.split_by_weights(&weights)?;

        let mut items = Vec::with_capacity(payable.len() + 2);
        for ((handle, wallet, contribution), amount) in payable.into_iter().zip(shares) {
            items.push(PayoutItem {
                kind: PayeeKind::Contributor,
                handle,
                wallet,
                amount,
                score: contribution.score,
                share_bps: bps_of(amount, self.gross),
            });
        }
        merge_duplicate_wallets(&mut items);

        // Fee recipients come last so contributor ordering is untouched.
        items.push(PayoutItem {
            kind: PayeeKind::Treasury,
            handle: "treasury".into(),
            wallet: self.config.wallets.treasury.clone(),
            amount: split.treasury,
            score: 0,
            share_bps: self.config.fees.treasury_bps as u32,
        });
        items.push(PayoutItem {
            kind: PayeeKind::Protocol,
            handle: self
                .config
                .project
                .open_collective
                .clone()
                .unwrap_or_else(|| "open-collective".into()),
            wallet: self.config.wallets.open_collective.clone(),
            amount: split.protocol,
            score: 0,
            share_bps: self.config.fees.protocol_bps as u32,
        });

        let mut plan = PayoutPlan {
            id: String::new(),
            project: self.config.project.name.clone(),
            created_at: self.created_at,
            asset: self.config.asset.clone(),
            range: self.range,
            split,
            items,
            unresolved,
        };
        plan.id = plan.compute_id();
        plan.verify()?;
        Ok(plan)
    }
}

/// Fold items sharing a wallet into one, summing amounts and scores.
fn merge_duplicate_wallets(items: &mut Vec<PayoutItem>) {
    let mut merged: Vec<PayoutItem> = Vec::with_capacity(items.len());
    for item in items.drain(..) {
        match merged
            .iter()
            .position(|existing: &PayoutItem| existing.wallet == item.wallet)
        {
            Some(index) => {
                let target = &mut merged[index];
                target.amount =
                    Amount::from_base_units(target.amount.base_units() + item.amount.base_units());
                target.score += item.score;
                target.share_bps += item.share_bps;
            }
            None => merged.push(item),
        }
    }
    *items = merged;
}

fn bps_of(part: Amount, whole: Amount) -> u32 {
    if whole.is_zero() {
        return 0;
    }
    ((part.base_units() * 10_000) / whole.base_units()) as u32
}

/// Seconds since the unix epoch.
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attribution::Contribution;
    use crate::git::Author;
    use crate::identity::Identity;

    fn contribution(email: &str, score: u128) -> Contribution {
        Contribution {
            author: Author::new(email, email),
            score,
            merges: 1,
            commits: 1,
            insertions: 10,
            deletions: 0,
        }
    }

    fn setup() -> (Config, Attribution) {
        let mut config = Config::template("dedalo");
        config.wallets.treasury = "0xtreasury".into();
        config.wallets.open_collective = "0xopencollective".into();
        config.identities = vec![
            Identity::new("ada", "0xada").with_email("ada@x.io"),
            Identity::new("bea", "0xbea").with_email("bea@x.io"),
        ];
        let attribution = Attribution {
            contributions: vec![
                contribution("ada@x.io", 3_000),
                contribution("bea@x.io", 1_000),
            ],
            merges_analysed: 2,
            total_score: 4_000,
        };
        (config, attribution)
    }

    fn range() -> PlanRange {
        PlanRange {
            branch: "main".into(),
            from_commit: None,
            to_commit: "deadbeef".into(),
            merges: 2,
        }
    }

    #[test]
    fn distributes_pool_by_weight_after_fees() {
        let (config, attribution) = setup();
        let gross = Amount::from_base_units(1_000_000);
        let plan = PlanBuilder::new(&config, &attribution, range(), gross)
            .created_at(0)
            .build()
            .unwrap();

        // 2.5% protocol, 15% treasury, 82.5% (825_000) split 3:1.
        let ada = plan.items.iter().find(|i| i.handle == "ada").unwrap();
        let bea = plan.items.iter().find(|i| i.handle == "bea").unwrap();
        assert_eq!(ada.amount.base_units(), 618_750);
        assert_eq!(bea.amount.base_units(), 206_250);
        assert_eq!(plan.total().unwrap(), gross);
        plan.verify().unwrap();
    }

    #[test]
    fn plan_id_is_deterministic_and_tamper_evident() {
        let (config, attribution) = setup();
        let gross = Amount::from_base_units(1_000_000);
        let first = PlanBuilder::new(&config, &attribution, range(), gross)
            .created_at(0)
            .build()
            .unwrap();
        let second = PlanBuilder::new(&config, &attribution, range(), gross)
            .created_at(999)
            .build()
            .unwrap();
        assert_eq!(first.id, second.id, "timestamp must not affect the id");

        let mut tampered = first.clone();
        tampered.items[0].amount = Amount::from_base_units(999_999);
        assert!(tampered.verify().is_err());
    }

    #[test]
    fn contributors_without_wallets_are_reported_not_dropped() {
        let (mut config, mut attribution) = setup();
        config.identities.pop();
        attribution
            .contributions
            .push(contribution("ghost@x.io", 500));
        attribution.total_score += 500;

        let plan = PlanBuilder::new(
            &config,
            &attribution,
            range(),
            Amount::from_base_units(1_000_000),
        )
        .created_at(0)
        .build()
        .unwrap();

        assert_eq!(plan.unresolved.len(), 2);
        assert!(plan.unresolved.iter().any(|u| u.email == "bea@x.io"));
        // The whole pool still goes out: no dust is stranded.
        assert_eq!(plan.total().unwrap().base_units(), 1_000_000);
    }

    #[test]
    fn one_wallet_receives_a_single_transfer() {
        let (mut config, mut attribution) = setup();
        config
            .identities
            .push(Identity::new("ada-work", "0xada").with_email("ada@work.io"));
        attribution
            .contributions
            .push(contribution("ada@work.io", 1_000));
        attribution.total_score += 1_000;

        let plan = PlanBuilder::new(
            &config,
            &attribution,
            range(),
            Amount::from_base_units(1_000_000),
        )
        .created_at(0)
        .build()
        .unwrap();

        assert_eq!(plan.items.iter().filter(|i| i.wallet == "0xada").count(), 1);
        plan.verify().unwrap();
    }
}
