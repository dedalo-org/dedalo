//! What the contract enforces, as ordinary Rust.
//!
//! This module is the contract. The deployable artifact at
//! `src/chain/contract` is a binding: it reads storage, calls the functions
//! below, and writes back what they return. Every rule that decides whether
//! money moves is here, where it can be tested exhaustively and read without
//! a second language.
//!
//! # Why the rules are pure
//!
//! A contract is the one part of this system that cannot be patched after it
//! ships. The usual way to test one is to deploy it to a simulated chain and
//! poke it, which tests the deployment as much as the rule. These functions
//! take the state they need and return the state they produce; nothing here
//! reads a clock, a balance or a caller. The result is that the rules can be
//! driven over their whole domain — see the exhaustive proofs — rather than
//! sampled with a handful of transactions.
//!
//! # The refusals are the specification
//!
//! [`Refusal`] has one variant per way this can say no, and every one of them
//! is a way money could otherwise be lost. Reading that enum is the shortest
//! description of what the vault guarantees.

mod round;

pub use round::Round;

use crate::chain::merkle::{ClaimTree, Hash, leaf_of};
use crate::money::Amount;

/// How long a round stays claimable, in seconds.
///
/// Fixed rather than chosen by the depositor: one who could choose it could
/// choose a window that closes before anybody claims.
pub const CLAIM_WINDOW: u64 = 180 * 24 * 60 * 60;

/// Every way the vault says no.
///
/// Each variant is a way funds could otherwise be lost or paid twice, so this
/// enum is the shortest statement of what the vault is for.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// This plan id has already funded a round. The replay guard: a retried
    /// job proposing the same plan cannot pay it a second time.
    #[error("this plan id was already deposited")]
    RoundExists,

    /// No round has been deposited for this plan id.
    #[error("no round for this plan id")]
    RoundUnknown,

    /// A round with no root, or nothing in it, can never be claimed — the
    /// money would go in and have no way out.
    #[error("a round needs a non-zero root and total")]
    NothingToDeposit,

    /// The token delivered less than the round promises. A fee-on-transfer
    /// token does this, and a round that promises more than it holds pays
    /// early claimants and strands the rest.
    #[error("the token delivered {delivered} of {expected}")]
    ShortDelivery {
        /// What actually arrived.
        delivered: Amount,
        /// What the round needs.
        expected: Amount,
    },

    /// This index of this round has already been paid.
    #[error("this index was already claimed")]
    AlreadyClaimed,

    /// The proof does not put this claim in this round's tree.
    #[error("the proof does not match the round's root")]
    BadProof,

    /// The claim is larger than what the round still holds.
    #[error("claim of {amount} exceeds the {remaining} still held")]
    ExceedsRound {
        /// What was claimed.
        amount: Amount,
        /// What is left.
        remaining: Amount,
    },

    /// The claim window has not closed yet.
    #[error("the claim window has not closed")]
    NotExpired,

    /// Only the account that funded a round may recover what is left of it.
    #[error("only the depositor may sweep")]
    NotDepositor,

    /// `claimed` exceeds `total`. Unreachable through these functions, and
    /// checked anyway: it means the state was written by something else.
    #[error("round accounting is inconsistent")]
    Inconsistent,

    /// Arithmetic would have wrapped.
    #[error("arithmetic overflow in the vault")]
    Overflow,
}

impl Refusal {
    /// A fixed sentence per refusal.
    ///
    /// [`Display`](core::fmt::Display) is what a person reads and interpolates
    /// the numbers; this is what a contract reverts with. Formatting a number
    /// on chain means linking the formatting machinery into the deployed
    /// artifact, and the artifact is paid for by the byte and capped at
    /// twenty-four kilobytes compressed.
    pub const fn reason(&self) -> &'static str {
        match self {
            Refusal::RoundExists => "dedalo: this plan id was already deposited",
            Refusal::RoundUnknown => "dedalo: no round for this plan id",
            Refusal::NothingToDeposit => "dedalo: a round needs a non-zero root and total",
            Refusal::ShortDelivery { .. } => {
                "dedalo: the token delivered less than the round needs"
            }
            Refusal::AlreadyClaimed => "dedalo: this index was already claimed",
            Refusal::BadProof => "dedalo: the proof does not match the round's root",
            Refusal::ExceedsRound { .. } => "dedalo: claim exceeds what the round still holds",
            Refusal::NotExpired => "dedalo: the claim window has not closed",
            Refusal::NotDepositor => "dedalo: only the depositor may sweep",
            Refusal::Inconsistent => "dedalo: round accounting is inconsistent",
            Refusal::Overflow => "dedalo: arithmetic overflow in the vault",
        }
    }
}

/// A deposit that was accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deposited {
    /// The round to store under the plan id.
    pub round: Round,
}

/// A claim that was accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paid {
    /// Where the transfer goes.
    pub account: [u8; 20],
    /// How much.
    pub amount: Amount,
    /// The round, with `claimed` advanced. Store this before transferring:
    /// a token with a transfer hook must not be able to re-enter and take
    /// the same index twice.
    pub round: Round,
}

/// A sweep that was accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Swept {
    /// Where the remainder goes. Always the depositor.
    pub account: [u8; 20],
    /// How much was left.
    pub amount: Amount,
    /// The round, marked fully claimed so a second sweep pays nothing.
    pub round: Round,
}

/// Decide whether a deposit may proceed.
///
/// `existing` is whatever is already stored under this plan id, `delivered`
/// is what the token actually moved, and `now` is the block timestamp.
///
/// # Errors
///
/// Every variant of [`Refusal`] this can return is a way the round would
/// otherwise be unclaimable or double-funded.
pub fn deposit(
    existing: Option<&Round>,
    root: Hash,
    token: [u8; 20],
    total: Amount,
    delivered: Amount,
    depositor: [u8; 20],
    now: u64,
) -> Result<Deposited, Refusal> {
    if existing.is_some() {
        return Err(Refusal::RoundExists);
    }
    if total == Amount::ZERO || root == [0u8; 32] {
        return Err(Refusal::NothingToDeposit);
    }
    // Measured, not assumed. The token is somebody else's code.
    if delivered < total {
        return Err(Refusal::ShortDelivery {
            delivered,
            expected: total,
        });
    }

    let expiry = now.checked_add(CLAIM_WINDOW).ok_or(Refusal::Overflow)?;

    Ok(Deposited {
        round: Round {
            root,
            token,
            total,
            claimed: Amount::ZERO,
            expiry,
            depositor,
        },
    })
}

/// Decide whether a claim may be paid.
///
/// `already_claimed` says whether this index of this round has been paid
/// before; the caller holds that set because a contract keeps it in storage
/// and a test keeps it in memory.
///
/// # Errors
///
/// See [`Refusal`].
pub fn claim(
    round: &Round,
    already_claimed: bool,
    index: u64,
    account: [u8; 20],
    amount: Amount,
    proof: &[Hash],
) -> Result<Paid, Refusal> {
    if already_claimed {
        return Err(Refusal::AlreadyClaimed);
    }
    // Re-established locally rather than assumed from a previous transaction:
    // the subtraction below depends on it, and a stored value is not a proof.
    if !round.is_consistent() {
        return Err(Refusal::Inconsistent);
    }

    if !ClaimTree::verify(round.root, leaf_of(index, account, amount), proof) {
        return Err(Refusal::BadProof);
    }

    let remaining = round.remaining().ok_or(Refusal::Inconsistent)?;
    if amount > remaining {
        return Err(Refusal::ExceedsRound { amount, remaining });
    }

    let mut updated = round.clone();
    updated.claimed = round
        .claimed
        .checked_add(amount)
        .map_err(|_| Refusal::Overflow)?;
    debug_assert!(updated.is_consistent(), "claim broke the invariant");

    Ok(Paid {
        account,
        amount,
        round: updated,
    })
}

/// Decide whether a sweep may return what nobody claimed.
///
/// # Errors
///
/// See [`Refusal`].
pub fn sweep(round: &Round, caller: &[u8; 20], now: u64) -> Result<Swept, Refusal> {
    if caller != &round.depositor {
        return Err(Refusal::NotDepositor);
    }
    if !round.has_expired(now) {
        return Err(Refusal::NotExpired);
    }
    if !round.is_consistent() {
        return Err(Refusal::Inconsistent);
    }

    let amount = round.remaining().ok_or(Refusal::Inconsistent)?;
    let mut updated = round.clone();
    updated.claimed = round.total;

    Ok(Swept {
        account: round.depositor,
        amount,
        round: updated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::merkle::Claim;
    use crate::chain::wallet::Address as DomainAddress;

    const ALICE: [u8; 20] = [0x11; 20];
    const BOB: [u8; 20] = [0x22; 20];
    const TOKEN: [u8; 20] = [0x33; 20];
    const NOW: u64 = 1_700_000_000;

    fn address(raw: [u8; 20]) -> DomainAddress {
        DomainAddress::from_evm_bytes(raw)
    }

    /// A round of three claims, and the tree that proves them.
    fn round_of(amounts: &[u128]) -> (ClaimTree, Round) {
        let claims: Vec<Claim> = amounts
            .iter()
            .enumerate()
            .map(|(index, amount)| Claim {
                index: index as u64,
                account: address([index as u8 + 1; 20]),
                amount: Amount::from_base_units(*amount),
            })
            .collect();
        let tree = ClaimTree::new(claims).unwrap();
        let total = tree.total().unwrap();
        let deposited = deposit(None, tree.root(), TOKEN, total, total, ALICE, NOW).unwrap();
        (tree, deposited.round)
    }

    fn claim_at(
        tree: &ClaimTree,
        round: &Round,
        index: usize,
        already: bool,
    ) -> Result<Paid, Refusal> {
        let entry = &tree.claims()[index];
        claim(
            round,
            already,
            entry.index,
            entry.account.evm_bytes().unwrap(),
            entry.amount,
            &tree.proof(index).unwrap(),
        )
    }

    /// The replay guard the whole idempotency story rests on.
    #[test]
    fn a_plan_id_funds_one_round_and_only_one() {
        let (tree, round) = round_of(&[100, 200, 300]);
        let again = deposit(
            Some(&round),
            tree.root(),
            TOKEN,
            round.total,
            round.total,
            ALICE,
            NOW,
        );
        assert_eq!(again, Err(Refusal::RoundExists));
    }

    #[test]
    fn a_round_with_nothing_in_it_is_refused() {
        let tree = ClaimTree::new(vec![Claim {
            index: 0,
            account: address(BOB),
            amount: Amount::from_base_units(1),
        }])
        .unwrap();

        assert_eq!(
            deposit(
                None,
                tree.root(),
                TOKEN,
                Amount::ZERO,
                Amount::ZERO,
                ALICE,
                NOW
            ),
            Err(Refusal::NothingToDeposit)
        );
        assert_eq!(
            deposit(
                None,
                [0u8; 32],
                TOKEN,
                Amount::from_base_units(1),
                Amount::from_base_units(1),
                ALICE,
                NOW
            ),
            Err(Refusal::NothingToDeposit)
        );
    }

    /// A fee-on-transfer token hands over less than it was asked for, and a
    /// round that promises more than it holds strands its last claimants.
    #[test]
    fn short_delivery_is_refused_at_the_deposit() {
        let tree = ClaimTree::new(vec![Claim {
            index: 0,
            account: address(BOB),
            amount: Amount::from_base_units(1_000),
        }])
        .unwrap();
        let total = Amount::from_base_units(1_000);
        let delivered = Amount::from_base_units(990);

        assert_eq!(
            deposit(None, tree.root(), TOKEN, total, delivered, ALICE, NOW),
            Err(Refusal::ShortDelivery {
                delivered,
                expected: total
            })
        );
    }

    #[test]
    fn every_claim_in_the_tree_is_paid_exactly_once() {
        let (tree, mut round) = round_of(&[100, 200, 300]);

        for index in 0..3 {
            let paid = claim_at(&tree, &round, index, false).unwrap();
            assert_eq!(paid.amount, tree.claims()[index].amount);
            round = paid.round;
        }
        assert_eq!(round.claimed, round.total, "the round is emptied exactly");
        assert_eq!(round.remaining().unwrap(), Amount::ZERO);

        // And a second attempt at any of them is refused.
        assert_eq!(
            claim_at(&tree, &round, 0, true),
            Err(Refusal::AlreadyClaimed)
        );
    }

    #[test]
    fn inflating_the_amount_breaks_the_proof() {
        let (tree, round) = round_of(&[100, 200, 300]);
        let entry = &tree.claims()[0];
        let refused = claim(
            &round,
            false,
            entry.index,
            entry.account.evm_bytes().unwrap(),
            Amount::from_base_units(999_999),
            &tree.proof(0).unwrap(),
        );
        assert_eq!(refused, Err(Refusal::BadProof));
    }

    #[test]
    fn redirecting_to_another_account_breaks_the_proof() {
        let (tree, round) = round_of(&[100, 200, 300]);
        let entry = &tree.claims()[0];
        let refused = claim(
            &round,
            false,
            entry.index,
            BOB,
            entry.amount,
            &tree.proof(0).unwrap(),
        );
        assert_eq!(refused, Err(Refusal::BadProof));
    }

    #[test]
    fn one_claims_proof_does_not_carry_another() {
        let (tree, round) = round_of(&[100, 200, 300]);
        let entry = &tree.claims()[0];
        let refused = claim(
            &round,
            false,
            entry.index,
            entry.account.evm_bytes().unwrap(),
            entry.amount,
            &tree.proof(1).unwrap(),
        );
        assert_eq!(refused, Err(Refusal::BadProof));
    }

    /// Cannot happen through a plan, whose items sum to its total. Checked
    /// anyway: the vault is the last thing between a bad root and a balance.
    #[test]
    fn a_claim_larger_than_the_round_is_refused() {
        let (tree, round) = round_of(&[100, 200, 300]);
        let entry = &tree.claims()[2];
        // A round that has already paid out all but a little.
        let nearly_empty = Round {
            claimed: round.total.checked_sub(Amount::from_base_units(1)).unwrap(),
            ..round.clone()
        };
        let refused = claim(
            &nearly_empty,
            false,
            entry.index,
            entry.account.evm_bytes().unwrap(),
            entry.amount,
            &tree.proof(2).unwrap(),
        );
        assert_eq!(
            refused,
            Err(Refusal::ExceedsRound {
                amount: entry.amount,
                remaining: Amount::from_base_units(1),
            })
        );
    }

    #[test]
    fn a_round_whose_accounting_is_broken_stops_rather_than_guesses() {
        let (tree, round) = round_of(&[100, 200, 300]);
        let broken = Round {
            claimed: round.total.checked_add(Amount::from_base_units(1)).unwrap(),
            ..round.clone()
        };
        assert!(!broken.is_consistent());
        assert_eq!(
            claim_at(&tree, &broken, 0, false),
            Err(Refusal::Inconsistent)
        );
        assert_eq!(
            sweep(&broken, &ALICE, NOW + CLAIM_WINDOW),
            Err(Refusal::Inconsistent)
        );
    }

    #[test]
    fn a_sweep_waits_for_the_window_and_only_answers_the_depositor() {
        let (_, round) = round_of(&[100, 200, 300]);

        assert_eq!(sweep(&round, &ALICE, NOW), Err(Refusal::NotExpired));
        assert_eq!(
            sweep(&round, &BOB, NOW + CLAIM_WINDOW),
            Err(Refusal::NotDepositor)
        );

        let swept = sweep(&round, &ALICE, NOW + CLAIM_WINDOW).unwrap();
        assert_eq!(swept.account, ALICE, "only ever the depositor");
        assert_eq!(swept.amount, round.total);
        assert_eq!(swept.round.remaining().unwrap(), Amount::ZERO);
    }

    /// Unclaimed money is recoverable, which is the difference between "not
    /// yet claimed" and "destroyed".
    #[test]
    fn a_sweep_returns_only_what_is_left() {
        let (tree, round) = round_of(&[100, 200, 300]);
        let after_one = claim_at(&tree, &round, 1, false).unwrap().round;

        let swept = sweep(&after_one, &ALICE, NOW + CLAIM_WINDOW).unwrap();
        assert_eq!(swept.amount, Amount::from_base_units(100 + 300));
        assert_eq!(swept.round.claimed, swept.round.total);

        // A second sweep pays nothing rather than paying again.
        let again = sweep(&swept.round, &ALICE, NOW + CLAIM_WINDOW).unwrap();
        assert_eq!(again.amount, Amount::ZERO);
    }

    #[test]
    fn a_timestamp_that_would_overflow_the_window_is_refused() {
        let tree = ClaimTree::new(vec![Claim {
            index: 0,
            account: address(BOB),
            amount: Amount::from_base_units(1),
        }])
        .unwrap();
        let one = Amount::from_base_units(1);
        assert_eq!(
            deposit(None, tree.root(), TOKEN, one, one, ALICE, u64::MAX),
            Err(Refusal::Overflow)
        );
    }

    /// Every refusal reverts with a fixed sentence, and no two share one.
    #[test]
    fn every_refusal_has_its_own_reason() {
        let all = [
            Refusal::RoundExists,
            Refusal::RoundUnknown,
            Refusal::NothingToDeposit,
            Refusal::ShortDelivery {
                delivered: Amount::ZERO,
                expected: Amount::ZERO,
            },
            Refusal::AlreadyClaimed,
            Refusal::BadProof,
            Refusal::ExceedsRound {
                amount: Amount::ZERO,
                remaining: Amount::ZERO,
            },
            Refusal::NotExpired,
            Refusal::NotDepositor,
            Refusal::Inconsistent,
            Refusal::Overflow,
        ];
        let reasons: std::collections::BTreeSet<&str> =
            all.iter().map(|refusal| refusal.reason()).collect();
        assert_eq!(reasons.len(), all.len(), "two refusals share a sentence");
        assert!(all.iter().all(|r| r.reason().starts_with("dedalo: ")));
    }
}
