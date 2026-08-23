//! Encoding the calls a signer executes.
//!
//! The call signatures are declared once, in Solidity syntax, and
//! [`alloy_sol_types`] generates the encoder from them. That is the same
//! library the rest of the Rust–Ethereum ecosystem uses, and the reason to
//! prefer it over the ninety lines it replaced is not the ninety lines: it is
//! that the two alignment rules the ABI has — `uintN` and `address`
//! right-aligned, `bytesN` left — are exactly the kind of detail a hand-rolled
//! encoder gets right until somebody edits it.
//!
//! The vault this targets is [`crate::chain::vault`], deployed as the Stylus
//! contract under `src/chain/contract`. A Stylus contract exposes an ordinary
//! Solidity ABI, so the calldata below is what any EVM tool would produce.

use alloy_sol_types::{SolCall, sol};

use crate::chain::wallet::Address;
use crate::error::{Error, Result};
use crate::money::Amount;

sol! {
    /// The ERC-20 allowance a deposit spends.
    function approve(address spender, uint256 amount) external returns (bool);

    /// Funding one round against the Merkle root of its payout plan.
    function deposit(bytes16 planId, bytes32 root, address token, uint256 total) external;

    /// Taking one share. Not encoded here — a contributor's wallet builds it —
    /// but declared so the signature this project commits to lives in one
    /// place, and so a change to it fails to compile rather than silently
    /// producing calldata nothing answers.
    function claim(
        bytes16 planId,
        uint256 index,
        address account,
        uint256 amount,
        bytes32[] proof
    ) external;
}

/// The sixteen bytes a plan id carries on chain.
///
/// A plan id is `ded1` and thirty-two hex digits — half a SHA-256. The tag is
/// for a person reading a file; the chain gets the hash.
///
/// # Errors
///
/// Returns [`Error::Config`] unless `plan_id` has exactly that shape. The same
/// validation the object store uses, for the same reason: an id is an
/// identifier and must never be able to act as anything else.
pub fn plan_id_bytes(plan_id: &str) -> Result<[u8; 16]> {
    crate::storage::objects::validate_id(plan_id, crate::storage::ledger::PLAN_TAG)?;
    let body = &plan_id[crate::storage::ledger::PLAN_TAG.len()..];
    let raw = hex::decode(body).map_err(|e| Error::config(format!("plan id is not hex: {e}")))?;
    raw.try_into()
        .map_err(|_| Error::config("a plan id carries sixteen bytes".to_string()))
}

fn evm(address: &Address) -> Result<alloy_sol_types::private::Address> {
    Ok(alloy_sol_types::private::Address::from(
        address.evm_bytes()?,
    ))
}

fn units(amount: Amount) -> alloy_sol_types::private::U256 {
    alloy_sol_types::private::U256::from(amount.base_units())
}

/// Calldata for approving the vault to move `amount` of the token.
pub fn approve_calldata(spender: &Address, amount: Amount) -> Result<Vec<u8>> {
    Ok(approveCall {
        spender: evm(spender)?,
        amount: units(amount),
    }
    .abi_encode())
}

/// Calldata for depositing a round against its Merkle root.
pub fn deposit_calldata(
    plan_id: &str,
    root: crate::chain::merkle::Hash,
    token: &Address,
    total: Amount,
) -> Result<Vec<u8>> {
    Ok(depositCall {
        planId: plan_id_bytes(plan_id)?.into(),
        root: root.into(),
        token: evm(token)?,
        total: units(total),
    }
    .abi_encode())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(byte: u8) -> Address {
        Address::from_evm_bytes([byte; 20])
    }

    /// The canonical vectors every ABI implementation is checked against.
    ///
    /// Kept even though the encoder is now a library's: they say what this
    /// project believes the selectors are, so swapping the library out cannot
    /// silently change what a signer is asked to sign.
    #[test]
    fn selectors_match_the_known_values() {
        assert_eq!(hex::encode(approveCall::SELECTOR), "095ea7b3");
        assert_eq!(
            hex::encode(depositCall::SELECTOR),
            hex::encode(
                &super::deposit_calldata(
                    "ded100112233445566778899aabbccddeeff",
                    [0u8; 32],
                    &addr(1),
                    Amount::ZERO
                )
                .unwrap()[..4]
            )
        );
    }

    /// The alignment rules, asserted against the bytes rather than trusted.
    #[test]
    fn numbers_are_right_aligned_and_byte_strings_are_left_aligned() {
        let data = deposit_calldata(
            "ded100112233445566778899aabbccddeeff",
            [0x7f; 32],
            &addr(0x11),
            Amount::from_base_units(1_000_000),
        )
        .unwrap();

        assert_eq!(data.len(), 4 + 4 * 32);
        // bytes16 left-aligned, then sixteen bytes of padding.
        assert_eq!(
            &data[4..20],
            &hex::decode("00112233445566778899aabbccddeeff").unwrap()[..]
        );
        assert!(
            data[20..36].iter().all(|b| *b == 0),
            "bytesN pads on the right"
        );
        // bytes32 fills its word.
        assert!(data[36..68].iter().all(|b| *b == 0x7f));
        // address right-aligned in the low twenty bytes.
        assert!(
            data[68..80].iter().all(|b| *b == 0),
            "address pads on the left"
        );
        assert!(data[80..100].iter().all(|b| *b == 0x11));
        // uint256 right-aligned.
        assert_eq!(
            u128::from_be_bytes(data[116..132].try_into().unwrap()),
            1_000_000
        );
    }

    #[test]
    fn a_plan_id_reaches_the_chain_as_its_sixteen_hash_bytes() {
        assert_eq!(
            plan_id_bytes("ded100112233445566778899aabbccddeeff").unwrap(),
            [
                0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
                0xee, 0xff
            ]
        );
        // An id is never a path, and never anything but an id.
        assert!(plan_id_bytes("../../etc/passwd").is_err());
        assert!(plan_id_bytes("ded1nothex0000000000000000000000000").is_err());
    }
}
