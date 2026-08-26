//! One funded round, as the vault holds it.

use serde::{Deserialize, Serialize};

use crate::chain::merkle::Hash;
use crate::money::Amount;

/// A round that has been deposited and can be claimed against.
///
/// `claimed <= total` is the invariant everything else rests on. It is not
/// enforced by the type — a struct cannot — but every function in
/// [`super`] that changes `claimed` re-establishes it, and
/// [`Round::is_consistent`] is what a caller checks when it did not build the
/// value itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Round {
    /// Root of the tree of `(index, account, amount)` leaves.
    pub root: Hash,
    /// Token mint being distributed, as the thirty-two bytes a chain holds.
    pub token: [u8; 32],
    /// Sum of every claim in the tree.
    pub total: Amount,
    /// How much has been claimed so far.
    pub claimed: Amount,
    /// When the claim window closes, as a unix timestamp.
    pub expiry: u64,
    /// Who funded it, and the only account a sweep may pay.
    pub depositor: [u8; 32],
}

impl Round {
    /// What is still claimable.
    ///
    /// Returns `None` rather than saturating when the invariant is broken:
    /// a vault whose accounting does not add up must stop, not guess.
    pub fn remaining(&self) -> Option<Amount> {
        self.total.checked_sub(self.claimed).ok()
    }

    /// Whether the accounting still holds.
    pub fn is_consistent(&self) -> bool {
        self.claimed <= self.total
    }

    /// Whether the claim window has closed at `now`.
    pub fn has_expired(&self, now: u64) -> bool {
        now >= self.expiry
    }
}
