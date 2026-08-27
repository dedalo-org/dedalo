//! Paying a wallet for holding a position, not for a merge.
//!
//! [`attribution`](crate::attribution) pays for work that landed. Some
//! contributions are not merges — maintaining CI, reviewing, answering issues,
//! holding a signing key — and the `roles` slice of a
//! [`RevenueSplit`](super::RevenueSplit) pays for those.
//!
//! # A role is a position, not an amount of work
//!
//! This is the line that keeps the module from becoming a second attribution
//! system with different rules. A weight says what a *position* is worth per
//! period; it says nothing about the person holding it and nothing about how
//! much they did. If a weight ever starts encoding effort, there are two
//! answers to "how much did you contribute" and they will disagree.
//!
//! So a role is held or not held. What varies is which positions exist and
//! what a project thinks each is worth, and both of those live in
//! `dedalo.toml` — a reviewed commit with an author, for the same reason the
//! fee schedule does.
//!
//! # One wallet, one payment
//!
//! A wallet holding two roles is paid once, for the sum of their weights. That
//! is the same invariant [`payout`](crate::payout) enforces for contributors,
//! and it is enforced here for the same reason: two entries paying one account
//! is both a double payment and a table that lies about how many people there
//! are.
//!
//! # What this module does not decide
//!
//! **When a period ends.** Nothing here reads a clock. A caller establishes
//! that a period is over and hands the slice to [`Distribution::for_period`];
//! this decides how it divides.
//!
//! That matters for idempotence, which is the hard part and is not solved
//! here: a distribution must be recorded in the ledger against a period
//! identifier, so running March twice pays once. The ledger already refuses a
//! repeated plan id and the same machinery carries this — see `#69`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::chain::wallet::Address;
use crate::error::{Error, Result};
use crate::money::Amount;

/// A position within a project, and what it is worth per period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Role {
    /// What the position is called. Appears in the distribution table.
    pub name: String,
    /// What the position is worth per period, relative to other roles.
    ///
    /// A bare number rather than a percentage: percentages of a set that
    /// changes have to be renormalised every time somebody joins, and a
    /// renormalisation is a chance to get it wrong.
    pub weight: u64,
    /// Wallets holding this position.
    pub wallets: Vec<Address>,
}

/// Everyone a project pays for holding a position.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Roles(pub Vec<Role>);

impl Roles {
    /// Reject a role set that cannot be paid or that pays nobody.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] for an unnamed role, a role with no wallets,
    /// or a wallet that is the zero address. A role with weight zero is
    /// allowed: it is how a project keeps a position on the books while it is
    /// unfunded, and it simply receives nothing.
    pub fn validate(&self) -> Result<()> {
        for role in &self.0 {
            if role.name.trim().is_empty() {
                return Err(Error::config("a role needs a name".to_string()));
            }
            if role.wallets.is_empty() {
                return Err(Error::config(format!(
                    "role `{}` has no wallets, so its weight would be paid to nobody",
                    role.name
                )));
            }
            for wallet in &role.wallets {
                if wallet.is_zero() {
                    return Err(Error::config(format!(
                        "role `{}` names the zero address, which destroys anything sent to it",
                        role.name
                    )));
                }
            }
        }
        Ok(())
    }

    /// Every wallet with a role, and the total weight it holds.
    ///
    /// A wallet appearing in two roles appears once here, carrying the sum.
    /// Ordered by address so the result is deterministic — the same role set
    /// must produce the same distribution on any machine, for the same reason
    /// a plan id must.
    pub fn weights(&self) -> BTreeMap<String, u128> {
        let mut totals: BTreeMap<String, u128> = BTreeMap::new();
        for role in &self.0 {
            for wallet in &role.wallets {
                *totals.entry(wallet.key()).or_default() += u128::from(role.weight);
            }
        }
        totals
    }
}

/// One wallet's share of a period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleShare {
    /// Where it goes.
    pub wallet: String,
    /// Total weight this wallet holds across every role.
    pub weight: u128,
    /// What it receives this period.
    pub amount: Amount,
}

/// What a period pays, to whom.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Distribution {
    /// The slice this period had to divide.
    pub pool: Amount,
    /// Every wallet with a role, in a deterministic order.
    pub shares: Vec<RoleShare>,
    /// What no wallet could be given.
    ///
    /// Non-zero only when every weight is zero — there is no rounding dust,
    /// because the split is largest-remainder and conserves exactly. It is
    /// stated rather than dropped, because money with nowhere to go is a fact
    /// somebody has to decide about.
    pub undistributed: Amount,
}

impl Distribution {
    /// Divide `pool` across every wallet holding a role.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] if the role set is invalid, and
    /// [`Error::Overflow`] if the weights cannot be summed.
    pub fn for_period(roles: &Roles, pool: Amount) -> Result<Self> {
        roles.validate()?;
        let totals = roles.weights();

        if totals.is_empty() || totals.values().all(|w| *w == 0) {
            // Nobody to pay, or nobody with a non-zero weight. The pool is
            // reported as undistributed rather than silently absorbed.
            return Ok(Self {
                pool,
                shares: Vec::new(),
                undistributed: pool,
            });
        }

        let wallets: Vec<&String> = totals.keys().collect();
        let weights: Vec<u128> = totals.values().copied().collect();
        let amounts = pool.split_by_weights(&weights)?;

        let shares = wallets
            .into_iter()
            .zip(weights.iter())
            .zip(amounts)
            .map(|((wallet, weight), amount)| RoleShare {
                wallet: wallet.clone(),
                weight: *weight,
                amount,
            })
            .collect();

        Ok(Self {
            pool,
            shares,
            undistributed: Amount::ZERO,
        })
    }

    /// Whether every base unit of the pool is accounted for.
    ///
    /// The same check [`RevenueSplit::balances`](super::RevenueSplit::balances)
    /// makes, for the same reason: it is the invariant, and an invariant
    /// nobody checks is a wish.
    pub fn balances(&self) -> bool {
        self.shares
            .iter()
            .try_fold(self.undistributed, |acc, share| {
                acc.checked_add(share.amount).ok()
            })
            .is_some_and(|total| total == self.pool)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wallet(tag: u8) -> Address {
        let mut raw = [tag; 32];
        for nudge in 0..=u8::MAX {
            raw[31] = nudge;
            let candidate = Address::from_pubkey_bytes(raw);
            if candidate.is_on_curve() {
                return candidate;
            }
        }
        unreachable!("some nudge of the last byte lands on the curve")
    }

    fn role(name: &str, weight: u64, wallets: &[Address]) -> Role {
        Role {
            name: name.into(),
            weight,
            wallets: wallets.to_vec(),
        }
    }

    #[test]
    fn weights_are_split_in_proportion_and_conserve_the_pool() {
        let (a, b) = (wallet(1), wallet(2));
        let roles = Roles(vec![
            role("maintainer", 100, std::slice::from_ref(&a)),
            role("reviewer", 25, std::slice::from_ref(&b)),
        ]);

        let d = Distribution::for_period(&roles, Amount::from_base_units(1_000)).unwrap();
        assert!(d.balances());
        assert_eq!(d.shares.len(), 2);

        let total: u128 = d.shares.iter().map(|s| s.amount.base_units()).sum();
        assert_eq!(total, 1_000);

        // 100:25 of 1000 is 800 and 200.
        let by_wallet: BTreeMap<&str, u128> = d
            .shares
            .iter()
            .map(|s| (s.wallet.as_str(), s.amount.base_units()))
            .collect();
        assert_eq!(by_wallet[a.key().as_str()], 800);
        assert_eq!(by_wallet[b.key().as_str()], 200);
    }

    #[test]
    fn one_wallet_holding_two_roles_is_paid_once_for_the_sum() {
        // The mirror of payout's invariant 5. Two entries paying one account
        // is a double payment and a table that lies about how many people
        // there are.
        let a = wallet(1);
        let roles = Roles(vec![
            role("maintainer", 60, std::slice::from_ref(&a)),
            role("signer", 40, std::slice::from_ref(&a)),
        ]);

        let d = Distribution::for_period(&roles, Amount::from_base_units(500)).unwrap();
        assert_eq!(d.shares.len(), 1, "one wallet, one payment");
        assert_eq!(d.shares[0].weight, 100);
        assert_eq!(d.shares[0].amount, Amount::from_base_units(500));
        assert!(d.balances());
    }

    #[test]
    fn a_pool_nobody_can_receive_is_declared_rather_than_absorbed() {
        let roles = Roles(vec![role("dormant", 0, &[wallet(3)])]);
        let d = Distribution::for_period(&roles, Amount::from_base_units(777)).unwrap();
        assert!(d.shares.is_empty());
        assert_eq!(d.undistributed, Amount::from_base_units(777));
        assert!(d.balances());

        // And with no roles at all.
        let d = Distribution::for_period(&Roles::default(), Amount::from_base_units(5)).unwrap();
        assert_eq!(d.undistributed, Amount::from_base_units(5));
        assert!(d.balances());
    }

    #[test]
    fn a_role_that_pays_nobody_or_pays_the_burn_address_is_refused() {
        let empty = Roles(vec![role("ghost", 10, &[])]);
        assert!(
            empty
                .validate()
                .unwrap_err()
                .to_string()
                .contains("no wallets")
        );

        let unnamed = Roles(vec![role("  ", 10, &[wallet(1)])]);
        assert!(unnamed.validate().is_err());

        let burn = Roles(vec![Role {
            name: "burn".into(),
            weight: 10,
            wallets: vec![Address::parse(crate::chain::wallet::ZERO_ADDRESS).unwrap()],
        }]);
        assert!(
            burn.validate()
                .unwrap_err()
                .to_string()
                .contains("zero address")
        );
    }

    #[test]
    fn the_same_roles_always_produce_the_same_distribution() {
        // Determinism is not decoration here: a distribution recorded in the
        // ledger has to be reproducible by anybody holding the same config,
        // exactly as a plan id is.
        let roles = Roles(vec![
            role("c", 3, &[wallet(9)]),
            role("a", 1, &[wallet(7)]),
            role("b", 2, &[wallet(8)]),
        ]);
        let first = Distribution::for_period(&roles, Amount::from_base_units(101)).unwrap();
        let second = Distribution::for_period(&roles, Amount::from_base_units(101)).unwrap();
        assert_eq!(first, second);

        // Declaration order must not matter either.
        let reordered = Roles(vec![
            role("a", 1, &[wallet(7)]),
            role("b", 2, &[wallet(8)]),
            role("c", 3, &[wallet(9)]),
        ]);
        let third = Distribution::for_period(&reordered, Amount::from_base_units(101)).unwrap();
        assert_eq!(first, third);
    }

    #[test]
    fn an_indivisible_pool_still_conserves_every_base_unit() {
        let roles = Roles(vec![
            role("a", 1, &[wallet(1)]),
            role("b", 1, &[wallet(2)]),
            role("c", 1, &[wallet(3)]),
        ]);
        // 1 base unit across three equal weights: largest-remainder gives it
        // to the first, and nothing is created or destroyed.
        let d = Distribution::for_period(&roles, Amount::from_base_units(1)).unwrap();
        assert!(d.balances());
        let total: u128 = d.shares.iter().map(|s| s.amount.base_units()).sum();
        assert_eq!(total, 1);
    }
}
