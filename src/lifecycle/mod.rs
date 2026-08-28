//! What a project's funding does as it grows.
//!
//! [`money::treasury`](crate::money::treasury) cuts one round. This module is
//! about the project across many of them: how much it has raised, what that
//! entitles it to do, and where the revenue goes once it does it.
//!
//! # Pure, for the same reason the vault is
//!
//! Nothing here reads a clock, a chain or a filesystem. A [`Stage`] is a
//! function of a number; a [`RevenueSchedule`] is a function of an amount.
//! That is what lets the whole domain be tested rather than sampled, and it is
//! the same split [`chain::vault`](crate::chain::vault) has — the rules are
//! ordinary Rust and the binding that reads accounts is somewhere else.
//!
//! So this module decides nothing about *when*. A caller establishes that a
//! period has ended or that a threshold has been crossed; this says what
//! follows.
//!
//! # The ladder
//!
//! A project accumulates funding, and what it may do changes as it does:
//!
//! ```text
//! Raising ──raised ≥ stake_threshold──▶ Staked ──raised ≥ token_threshold──▶ Tokenised
//! ```
//!
//! Thresholds are configuration. A project decides its own, and a project that
//! sets neither stays [`Stage::Raising`] forever, which is a valid way to use
//! this tool.
//!
//! **The ladder only goes up.** [`Ladder::stage_for`] is monotonic in the
//! amount raised, and `raised` is a lifetime total that never decreases —
//! spending the reserve does not un-tokenise a project, because a token that
//! could be withdrawn by spending is not a token anybody would hold.
//!
//! # Where revenue goes
//!
//! Once tokenised, transfers of the token carry a fee (see the design in
//! `#67`). Harvested fees are cut four ways by [`RevenueSchedule`], in a fixed
//! order and in basis points, and the arithmetic obeys the same two rules the
//! fee schedule does:
//!
//! - **every base unit is accounted for** — the four slices sum to exactly the
//!   input, always;
//! - **slices round down and the remainder goes to contributors**, never the
//!   other way round.
//!
//! # Status
//!
//! Nothing here is wired to a chain, because the claim program does not exist.
//! What this is, is the specification such a program would be checked against
//! — which is exactly what `chain::vault` was before it had a binding, and it
//! is worth more than a binding written first and reasoned about after.

#[cfg(test)]
mod proofs;
pub mod roles;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::money::Amount;
use crate::money::treasury::BPS_DENOMINATOR;

/// What a project is entitled to do, given what it has raised.
///
/// Ordered, and comparable: `Raising < Staked < Tokenised`. The ordering is
/// meaningful rather than incidental — a stage grants everything the stages
/// below it grant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    /// Taking contributions and paying contributors. Where every project
    /// starts, and where a project with no thresholds set stays.
    #[default]
    Raising,
    /// Above the staking threshold: a share of the reserve may be staked.
    Staked,
    /// Above the token threshold: the project may mint its token.
    Tokenised,
}

impl Stage {
    /// How to describe this stage to somebody reading terminal output.
    pub fn description(self) -> &'static str {
        match self {
            Stage::Raising => "raising — contributions accumulate, rounds pay contributors",
            Stage::Staked => "staked — a share of the reserve may be staked",
            Stage::Tokenised => "tokenised — the project may mint and trade its token",
        }
    }
}

/// The amounts at which a project changes stage.
///
/// `None` means the stage is unreachable, which is the default: a project opts
/// in to each rung by naming a number, and a project that names none simply
/// keeps paying contributors.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ladder {
    /// Lifetime raised at which staking becomes available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stake_threshold: Option<Amount>,
    /// Lifetime raised at which the project may mint its token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_threshold: Option<Amount>,
}

impl Ladder {
    /// Reject a ladder whose rungs are in the wrong order.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] if the token threshold is below the staking
    /// one. That would let a project mint a token it cannot stake behind, and
    /// more to the point it means somebody typed one of the two numbers wrong
    /// — which is worth saying rather than silently reordering.
    pub fn validate(&self) -> Result<()> {
        if let (Some(stake), Some(token)) = (self.stake_threshold, self.token_threshold)
            && token < stake
        {
            return Err(Error::config(format!(
                "token_threshold ({}) is below stake_threshold ({}); a project cannot \
                 tokenise before it can stake",
                token.base_units(),
                stake.base_units()
            )));
        }
        Ok(())
    }

    /// The stage a project with this lifetime total has reached.
    ///
    /// Monotonic in `raised`: more raised never means a lower stage. A
    /// threshold is inclusive — reaching it exactly is reaching it.
    pub fn stage_for(&self, raised: Amount) -> Stage {
        let reached = |threshold: Option<Amount>| threshold.is_some_and(|t| raised >= t);

        // Checked highest first: the token rung implies the one below it even
        // if a project set only the higher number.
        if reached(self.token_threshold) {
            Stage::Tokenised
        } else if reached(self.stake_threshold) {
            Stage::Staked
        } else {
            Stage::Raising
        }
    }

    /// What is still needed to reach the next stage, if there is one.
    ///
    /// `None` at the top of the ladder, or where the next rung was never
    /// configured. Reported so a project can be told how far off it is rather
    /// than only which side of a line it is on.
    pub fn remaining_to_next(&self, raised: Amount) -> Option<Amount> {
        let next = match self.stage_for(raised) {
            Stage::Raising => self.stake_threshold.or(self.token_threshold),
            Stage::Staked => self.token_threshold,
            Stage::Tokenised => None,
        }?;
        next.checked_sub(raised).ok()
    }
}

/// How harvested token revenue is divided.
///
/// Three slices are named and the fourth is the remainder, exactly as
/// [`FeeSchedule`](crate::money::treasury::FeeSchedule) works: the named
/// slices round down and what is left goes to contributors, so rounding never
/// costs the people who wrote the code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RevenueSchedule {
    /// Back into the project's reserve — the self-funding loop.
    pub refinance_bps: u16,
    /// Added to the staked position.
    pub staking_bps: u16,
    /// Distributed periodically to wallets by role weight.
    pub roles_bps: u16,
}

impl Default for RevenueSchedule {
    fn default() -> Self {
        // 40% back into the reserve, 20% staked, 10% to roles, 30% to
        // contributors. Chosen so the contributor slice is the largest single
        // destination that is not the project itself, and so that the two
        // slices which compound — reserve and stake — are together a majority.
        //
        // These are defaults for a template, not a recommendation anybody has
        // modelled. A project should choose its own and say why.
        Self {
            refinance_bps: 4_000,
            staking_bps: 2_000,
            roles_bps: 1_000,
        }
    }
}

impl RevenueSchedule {
    /// Reject a schedule that would leave contributors with nothing.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] if the named slices consume the whole
    /// input. Contributors are the remainder, and a remainder of zero is a
    /// schedule that pays the project out of the work and gives none back.
    pub fn validate(&self) -> Result<()> {
        let total = self.refinance_bps as u32 + self.staking_bps as u32 + self.roles_bps as u32;
        if total >= BPS_DENOMINATOR {
            return Err(Error::config(format!(
                "revenue slices consume {total} bps of {BPS_DENOMINATOR}; contributors \
                 would receive nothing"
            )));
        }
        Ok(())
    }

    /// What is left for contributors, in basis points.
    pub fn contributor_bps(&self) -> u16 {
        (BPS_DENOMINATOR
            - self.refinance_bps as u32
            - self.staking_bps as u32
            - self.roles_bps as u32) as u16
    }

    /// Cut harvested revenue into its four destinations.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] if the schedule is invalid, and
    /// [`Error::Overflow`] if the arithmetic cannot be done — neither of which
    /// can happen for a validated schedule and a real amount, and both of
    /// which are returned rather than panicked because this decides money.
    pub fn split(&self, harvested: Amount) -> Result<RevenueSplit> {
        self.validate()?;
        let refinance = harvested.bps(self.refinance_bps)?;
        let staking = harvested.bps(self.staking_bps)?;
        let roles = harvested.bps(self.roles_bps)?;
        // Subtraction rather than a fourth multiplication: this is what makes
        // the four slices sum to exactly the input for every input, instead of
        // for most of them.
        let contributors = harvested
            .checked_sub(refinance)?
            .checked_sub(staking)?
            .checked_sub(roles)?;
        Ok(RevenueSplit {
            harvested,
            refinance,
            staking,
            roles,
            contributors,
        })
    }
}

/// The result of applying a [`RevenueSchedule`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevenueSplit {
    /// Everything that was harvested, before any cut.
    pub harvested: Amount,
    /// Back into the reserve.
    pub refinance: Amount,
    /// Added to the staked position.
    pub staking: Amount,
    /// Held for the next periodic role distribution.
    pub roles: Amount,
    /// Held for contributors, on top of what rounds already pay.
    pub contributors: Amount,
}

impl RevenueSplit {
    /// Whether the four slices sum to exactly what was harvested.
    ///
    /// Checked rather than trusted, and checked by callers before anything is
    /// written down. The invariant is the whole point of the module.
    pub fn balances(&self) -> bool {
        [self.refinance, self.staking, self.roles, self.contributors]
            .iter()
            .try_fold(Amount::ZERO, |acc, slice| acc.checked_add(*slice).ok())
            .is_some_and(|total| total == self.harvested)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn amount(units: u128) -> Amount {
        Amount::from_base_units(units)
    }

    #[test]
    fn a_project_with_no_thresholds_stays_raising_forever() {
        let ladder = Ladder::default();
        assert_eq!(ladder.stage_for(Amount::ZERO), Stage::Raising);
        assert_eq!(ladder.stage_for(amount(u64::MAX as u128)), Stage::Raising);
        assert_eq!(ladder.remaining_to_next(Amount::ZERO), None);
    }

    #[test]
    fn a_threshold_is_reached_by_meeting_it_exactly() {
        let ladder = Ladder {
            stake_threshold: Some(amount(1_000)),
            token_threshold: Some(amount(5_000)),
        };
        assert_eq!(ladder.stage_for(amount(999)), Stage::Raising);
        assert_eq!(ladder.stage_for(amount(1_000)), Stage::Staked);
        assert_eq!(ladder.stage_for(amount(4_999)), Stage::Staked);
        assert_eq!(ladder.stage_for(amount(5_000)), Stage::Tokenised);
    }

    #[test]
    fn the_token_rung_implies_the_one_below_it() {
        // A project that named only the higher number is tokenised, not stuck
        // in Raising because the rung underneath was never configured.
        let ladder = Ladder {
            stake_threshold: None,
            token_threshold: Some(amount(100)),
        };
        assert_eq!(ladder.stage_for(amount(100)), Stage::Tokenised);
        assert!(Stage::Tokenised > Stage::Staked);
        assert!(Stage::Staked > Stage::Raising);
    }

    #[test]
    fn a_ladder_whose_rungs_are_upside_down_is_refused() {
        let wrong = Ladder {
            stake_threshold: Some(amount(5_000)),
            token_threshold: Some(amount(1_000)),
        };
        let error = wrong.validate().unwrap_err().to_string();
        assert!(error.contains("below stake_threshold"), "{error}");

        // Equal is allowed: one rung, reached at one number.
        let together = Ladder {
            stake_threshold: Some(amount(1_000)),
            token_threshold: Some(amount(1_000)),
        };
        assert!(together.validate().is_ok());
        assert_eq!(together.stage_for(amount(1_000)), Stage::Tokenised);
    }

    #[test]
    fn remaining_counts_down_and_stops_at_the_top() {
        let ladder = Ladder {
            stake_threshold: Some(amount(1_000)),
            token_threshold: Some(amount(5_000)),
        };
        assert_eq!(ladder.remaining_to_next(amount(400)), Some(amount(600)));
        assert_eq!(ladder.remaining_to_next(amount(1_000)), Some(amount(4_000)));
        assert_eq!(ladder.remaining_to_next(amount(5_000)), None);
    }

    #[test]
    fn the_four_slices_sum_to_exactly_what_was_harvested() {
        let schedule = RevenueSchedule::default();
        for units in [0u128, 1, 2, 3, 7, 99, 100, 101, 9_999, 10_000, 10_001] {
            let split = schedule.split(amount(units)).unwrap();
            assert!(split.balances(), "{units} base units did not balance");
        }
    }

    #[test]
    fn rounding_dust_goes_to_contributors_and_never_to_the_project() {
        // 1 base unit cannot be divided, so every named slice rounds to zero
        // and the whole unit must land with the people who wrote the code.
        let split = RevenueSchedule::default().split(amount(1)).unwrap();
        assert_eq!(split.refinance, Amount::ZERO);
        assert_eq!(split.staking, Amount::ZERO);
        assert_eq!(split.roles, Amount::ZERO);
        assert_eq!(split.contributors, amount(1));
        assert!(split.balances());
    }

    #[test]
    fn a_schedule_that_leaves_contributors_nothing_is_refused() {
        let greedy = RevenueSchedule {
            refinance_bps: 9_000,
            staking_bps: 900,
            roles_bps: 100,
        };
        let error = greedy.validate().unwrap_err().to_string();
        assert!(error.contains("contributors"), "{error}");
        assert!(greedy.split(amount(1_000)).is_err());

        // One basis point short of everything is allowed, and leaves one.
        let barely = RevenueSchedule {
            refinance_bps: 9_000,
            staking_bps: 900,
            roles_bps: 99,
        };
        assert!(barely.validate().is_ok());
        assert_eq!(barely.contributor_bps(), 1);
    }

    #[test]
    fn the_default_schedule_is_the_one_the_docs_claim() {
        let schedule = RevenueSchedule::default();
        assert_eq!(schedule.contributor_bps(), 3_000);
        let split = schedule.split(amount(10_000)).unwrap();
        assert_eq!(split.refinance, amount(4_000));
        assert_eq!(split.staking, amount(2_000));
        assert_eq!(split.roles, amount(1_000));
        assert_eq!(split.contributors, amount(3_000));
    }

    /// Every stage can say what it is, and says something different.
    ///
    /// These strings are what a maintainer reads to find out what their
    /// project is currently allowed to do. Two stages describing themselves
    /// identically would be a silent copy-paste, which is exactly the kind of
    /// thing a test catches and a reviewer does not.
    #[test]
    fn every_stage_describes_itself_and_no_two_agree() {
        let stages = [Stage::Raising, Stage::Staked, Stage::Tokenised];
        let descriptions: Vec<&str> = stages.iter().map(|s| s.description()).collect();

        for (stage, description) in stages.iter().zip(&descriptions) {
            assert!(
                !description.is_empty(),
                "{stage:?} describes itself as nothing"
            );
        }

        let mut unique = descriptions.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), stages.len(), "two stages share a description");
    }
}
