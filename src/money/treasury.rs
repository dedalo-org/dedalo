//! Where the money is cut before contributors are paid.
//!
//! Every round is sliced in a fixed order: the protocol fee that funds the
//! network's own Open Collective, then the project's own treasury reserve,
//! and whatever is left is the contributor pool. Fees are taken off the top
//! so the network is funded by the same flow it enables — that is what makes
//! it self-sustaining rather than dependent on grants.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::money::Amount;

/// Basis points: 10_000 bps == 100%.
pub const BPS_DENOMINATOR: u32 = 10_000;

/// The share of each round taken before contributors are paid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FeeSchedule {
    /// Share routed to the Open Collective wallet that funds the network.
    pub protocol_bps: u16,
    /// Share retained by the project for future rounds, audits, infra.
    pub treasury_bps: u16,
}

impl Default for FeeSchedule {
    fn default() -> Self {
        // 2.5% protocol / 15% project reserve / 82.5% contributors.
        Self {
            protocol_bps: 250,
            treasury_bps: 1_500,
        }
    }
}

impl FeeSchedule {
    /// Reject a schedule that would leave contributors with nothing.
    pub fn validate(&self) -> Result<()> {
        let total = self.protocol_bps as u32 + self.treasury_bps as u32;
        if total >= BPS_DENOMINATOR {
            return Err(Error::config(format!(
                "fees consume {total} bps of 10000; contributors would receive nothing"
            )));
        }
        Ok(())
    }

    /// What is left for contributors, in basis points.
    pub fn contributor_bps(&self) -> u16 {
        (BPS_DENOMINATOR - self.protocol_bps as u32 - self.treasury_bps as u32) as u16
    }

    /// Cut `gross` into protocol fee, treasury reserve and contributor pool.
    ///
    /// Fees round down, so any rounding dust lands in the contributor pool
    /// rather than in the network's pocket.
    pub fn split(&self, gross: Amount) -> Result<TreasurySplit> {
        self.validate()?;
        let protocol = gross.bps(self.protocol_bps)?;
        let treasury = gross.bps(self.treasury_bps)?;
        let contributors = gross.checked_sub(protocol)?.checked_sub(treasury)?;
        Ok(TreasurySplit {
            gross,
            protocol,
            treasury,
            contributors,
        })
    }
}

/// The result of applying a [`FeeSchedule`] to a gross amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreasurySplit {
    /// The full size of the round, before any cut.
    pub gross: Amount,
    /// Goes to the network's Open Collective wallet.
    pub protocol: Amount,
    /// Goes to the project's own reserve.
    pub treasury: Amount,
    /// Distributed across contributors by attribution weight.
    pub contributors: Amount,
}

impl TreasurySplit {
    /// Invariant every plan is checked against before it can be settled.
    pub fn is_balanced(&self) -> bool {
        self.protocol
            .checked_add(self.treasury)
            .and_then(|partial| partial.checked_add(self.contributors))
            .map(|sum| sum == self.gross)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_conserves_the_gross_amount() {
        let fees = FeeSchedule::default();
        let split = fees.split(Amount::from_base_units(1_000_000)).unwrap();
        assert_eq!(split.protocol.base_units(), 25_000);
        assert_eq!(split.treasury.base_units(), 150_000);
        assert_eq!(split.contributors.base_units(), 825_000);
        assert!(split.is_balanced());
    }

    #[test]
    fn rounding_dust_goes_to_contributors() {
        let fees = FeeSchedule {
            protocol_bps: 333,
            treasury_bps: 777,
        };
        let split = fees.split(Amount::from_base_units(101)).unwrap();
        assert!(split.is_balanced());
        assert_eq!(split.protocol.base_units(), 3);
        assert_eq!(split.treasury.base_units(), 7);
        assert_eq!(split.contributors.base_units(), 91);
    }

    #[test]
    fn rejects_fees_that_starve_contributors() {
        let fees = FeeSchedule {
            protocol_bps: 5_000,
            treasury_bps: 5_000,
        };
        assert!(fees.validate().is_err());
        assert!(fees.split(Amount::from_base_units(10)).is_err());
    }
}
