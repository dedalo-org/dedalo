//! The claim vault, deployable to Arbitrum Stylus.
//!
//! This crate is a **binding**, not an implementation. Every rule that decides
//! whether money moves lives in [`dedalo::chain::vault`], where it is ordinary
//! Rust: no storage, no clock, no caller, and therefore testable over its
//! whole domain rather than by deploying it somewhere and poking it.
//!
//! What is here is the part that cannot be pure — reading storage, moving a
//! token, and knowing what time it is — and it is kept as thin as that
//! description suggests. A reader checking whether this contract is correct
//! should end up reading `vault`, and should find nothing here that changes
//! the answer.
//!
//! # Status
//!
//! **Unaudited and undeployed.** Nothing here has held a coin. See
//! `docs/settlement-architecture.md` for what has to exist first.

#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]

extern crate alloc;

use alloc::vec::Vec;

use dedalo::chain::merkle::Hash;
use dedalo::chain::vault::{self, Refusal, Round};
use dedalo::money::Amount;

use stylus_sdk::alloy_primitives::{Address, FixedBytes, U256};
use stylus_sdk::prelude::*;

sol_storage! {
    /// One round per plan id, and the set of indices already paid.
    #[entrypoint]
    pub struct DedaloVault {
        mapping(bytes16 => StoredRound) rounds;
        mapping(bytes16 => mapping(uint256 => bool)) claimed;
    }

    /// A round, flattened into what storage can hold.
    ///
    /// `depositor` doubles as the "exists" flag: the zero address is not a
    /// depositor, so a round nobody funded is distinguishable from one that
    /// was funded and fully claimed.
    pub struct StoredRound {
        bytes32 root;
        address token;
        uint256 total;
        uint256 claimed;
        uint256 expiry;
        address depositor;
    }
}

sol_interface! {
    interface IErc20 {
        function transfer(address to, uint256 amount) external returns (bool);
        function transferFrom(address from, address to, uint256 amount) external returns (bool);
        function balanceOf(address who) external view returns (uint256);
    }
}

/// Everything this contract can refuse, as a revert reason.
///
/// One variant per [`Refusal`], plus the two failures that only exist at this
/// boundary: a token that will not move, and a value too large for the
/// arithmetic the rules are written in.
#[derive(SolidityError)]
pub enum VaultError {
    /// A rule in [`dedalo::chain::vault`] said no.
    Refused(Refused),
    /// The token call failed, or returned false.
    TokenRefused(TokenRefused),
    /// A `uint256` that does not fit the vault's arithmetic.
    AmountTooLarge(AmountTooLarge),
}

alloy_sol_types::sol! {
    error Refused(string reason);
    error TokenRefused();
    error AmountTooLarge();
}

impl From<Refusal> for VaultError {
    fn from(refusal: Refusal) -> Self {
        // The rule's own sentence, but the static one: formatting a number
        // here would link the whole formatting machinery into a wasm that is
        // capped at twenty-four kilobytes compressed.
        VaultError::Refused(Refused {
            reason: refusal.reason().into(),
        })
    }
}

/// Amounts are `u128` in the rules and `uint256` on chain.
///
/// Refused rather than truncated. A token supply above `u128::MAX` is
/// implausible — it is 3.4e38 base units — but "implausible" is not a reason
/// to silently pay the low bits.
fn to_amount(value: U256) -> Result<Amount, VaultError> {
    let limbs: [u64; 4] = value.into_limbs();
    if limbs[2] != 0 || limbs[3] != 0 {
        return Err(VaultError::AmountTooLarge(AmountTooLarge {}));
    }
    Ok(Amount::from_base_units(
        u128::from(limbs[0]) | (u128::from(limbs[1]) << 64),
    ))
}

fn from_amount(amount: Amount) -> U256 {
    U256::from(amount.base_units())
}

/// The vault holds twenty bytes, which is what this chain holds too, so the
/// boundary is a move rather than a conversion.
fn raw(address: Address) -> [u8; 20] {
    address.into_array()
}

#[public]
impl DedaloVault {
    /// Fund a round against the Merkle root of its payout plan.
    ///
    /// The delivered amount is measured rather than assumed: a fee-on-transfer
    /// token hands over less than it was asked for, and a round that promises
    /// more than it holds pays early claimants and strands the rest.
    pub fn deposit(
        &mut self,
        plan_id: FixedBytes<16>,
        root: FixedBytes<32>,
        token: Address,
        total: U256,
    ) -> Result<(), VaultError> {
        let existing = self.stored_round(plan_id);
        let contract = self.vm().contract_address();
        let sender = self.vm().msg_sender();
        let erc20 = IErc20::new(token);

        let before = erc20
            .balance_of(self.vm(), Call::new(), contract)
            .map_err(|_| VaultError::TokenRefused(TokenRefused {}))?;

        // The mutable borrow ends when `new_mutating` returns — the context it
        // produces holds nothing — so it has to be taken before `vm()` takes
        // an immutable one.
        let context = Call::new_mutating(self);
        let moved = erc20
            .transfer_from(self.vm(), context, sender, contract, total)
            .map_err(|_| VaultError::TokenRefused(TokenRefused {}))?;
        if !moved {
            return Err(VaultError::TokenRefused(TokenRefused {}));
        }

        let after = erc20
            .balance_of(self.vm(), Call::new(), contract)
            .map_err(|_| VaultError::TokenRefused(TokenRefused {}))?;
        let delivered = to_amount(after.saturating_sub(before))?;

        let decision = vault::deposit(
            existing.as_ref(),
            Hash::from(root.0),
            raw(token),
            to_amount(total)?,
            delivered,
            raw(sender),
            self.vm().block_timestamp(),
        )?;

        self.write_round(plan_id, &decision.round)?;
        Ok(())
    }

    /// Take one share of a round.
    ///
    /// Anyone may submit the transaction; the funds always go to `account`.
    /// That lets a project pay a contributor's gas without being able to
    /// redirect the money.
    pub fn claim(
        &mut self,
        plan_id: FixedBytes<16>,
        index: U256,
        account: Address,
        amount: U256,
        proof: Vec<FixedBytes<32>>,
    ) -> Result<(), VaultError> {
        let round = self.stored_round(plan_id).ok_or(Refusal::RoundUnknown)?;
        let already = self.claimed.getter(plan_id).get(index);

        let siblings: Vec<Hash> = proof.iter().map(|node| Hash::from(node.0)).collect();
        let index_u64 = to_amount(index)?.base_units() as u64;

        let paid = vault::claim(
            &round,
            already,
            index_u64,
            raw(account),
            to_amount(amount)?,
            &siblings,
        )?;

        // Effects before interaction. A token with a transfer hook must not be
        // able to re-enter and take the same index twice.
        self.claimed.setter(plan_id).setter(index).set(true);
        self.write_round(plan_id, &paid.round)?;

        self.pay(round.token, Address::from(paid.account), paid.amount)
    }

    /// After the window closes, return what nobody claimed.
    pub fn sweep(&mut self, plan_id: FixedBytes<16>) -> Result<(), VaultError> {
        let round = self.stored_round(plan_id).ok_or(Refusal::RoundUnknown)?;
        let caller = raw(self.vm().msg_sender());

        let swept = vault::sweep(&round, &caller, self.vm().block_timestamp())?;

        self.write_round(plan_id, &swept.round)?;
        if swept.amount == Amount::ZERO {
            return Ok(());
        }
        self.pay(round.token, Address::from(swept.account), swept.amount)
    }

    /// What a round still holds, for a caller deciding whether to claim.
    pub fn remaining(&self, plan_id: FixedBytes<16>) -> Result<U256, VaultError> {
        let round = self.stored_round(plan_id).ok_or(Refusal::RoundUnknown)?;
        let left = round.remaining().ok_or(Refusal::Inconsistent)?;
        Ok(from_amount(left))
    }

    /// Whether an index of a round has already been paid.
    pub fn is_claimed(&self, plan_id: FixedBytes<16>, index: U256) -> bool {
        self.claimed.getter(plan_id).get(index)
    }
}

impl DedaloVault {
    /// Read a round out of storage, or `None` when nobody funded one.
    fn stored_round(&self, plan_id: FixedBytes<16>) -> Option<Round> {
        let stored = self.rounds.getter(plan_id);
        let depositor = stored.depositor.get();
        if depositor == Address::ZERO {
            return None;
        }
        Some(Round {
            root: Hash::from(stored.root.get().0),
            token: raw(stored.token.get()),
            total: to_amount(stored.total.get()).ok()?,
            claimed: to_amount(stored.claimed.get()).ok()?,
            expiry: stored.expiry.get().to::<u64>(),
            depositor: raw(depositor),
        })
    }

    fn write_round(&mut self, plan_id: FixedBytes<16>, round: &Round) -> Result<(), VaultError> {
        let mut slot = self.rounds.setter(plan_id);
        slot.root.set(FixedBytes::from(round.root));
        slot.token.set(Address::from(round.token));
        slot.total.set(from_amount(round.total));
        slot.claimed.set(from_amount(round.claimed));
        slot.expiry.set(U256::from(round.expiry));
        slot.depositor.set(Address::from(round.depositor));
        Ok(())
    }

    fn pay(&mut self, token: [u8; 20], to: Address, amount: Amount) -> Result<(), VaultError> {
        let erc20 = IErc20::new(Address::from(token));
        let value = from_amount(amount);
        let context = Call::new_mutating(self);
        let sent = erc20
            .transfer(self.vm(), context, to, value)
            .map_err(|_| VaultError::TokenRefused(TokenRefused {}))?;
        if sent {
            Ok(())
        } else {
            Err(VaultError::TokenRefused(TokenRefused {}))
        }
    }
}
