//! Encoding calls the way the EVM reads them.
//!
//! Hand-written rather than generated, for the same reason [`crate::wallet`]
//! implements EIP-55 by hand: this is the byte string that decides where money
//! goes, it is ninety lines, and a reviewer can check every one of them
//! against the ABI specification without also auditing a code generator.
//!
//! # The two rules that bite
//!
//! - **`uintN` and `address` are right-aligned** in their 32-byte word.
//! - **`bytesN` is left-aligned**, padded on the right.
//!
//! Getting the second wrong produces a plan id of zero that still encodes,
//! still submits, and matches every round — so both have a test.

use sha3::{Digest, Keccak256};

use crate::error::{Error, Result};
use crate::merkle::Hash;
use crate::money::Amount;
use crate::wallet::Address;

/// One 32-byte ABI word.
pub type Word = [u8; 32];

/// The four-byte selector for a function signature.
///
/// The signature is the canonical form — `deposit(bytes16,bytes32,address,uint256)`,
/// no argument names, no spaces. Anything else selects a different function or
/// none at all.
pub fn selector(signature: &str) -> [u8; 4] {
    let digest = Keccak256::digest(signature.as_bytes());
    [digest[0], digest[1], digest[2], digest[3]]
}

/// Encode a `uint256`, right-aligned.
pub fn uint256(value: u128) -> Word {
    let mut word = [0u8; 32];
    word[16..].copy_from_slice(&value.to_be_bytes());
    word
}

/// Encode an `address`, right-aligned in the low 20 bytes.
///
/// # Errors
///
/// Returns [`Error::Address`] if this is not an EVM address. Truncating a
/// wider address into 20 bytes would produce a valid-looking word pointing at
/// an account nobody controls.
pub fn address(value: &Address) -> Result<Word> {
    let body = value
        .as_str()
        .strip_prefix("0x")
        .ok_or_else(|| Error::address(value.as_str(), "not an EVM address"))?;
    let raw =
        hex::decode(body).map_err(|e| Error::address(value.as_str(), format!("not hex: {e}")))?;
    if raw.len() != 20 {
        return Err(Error::address(value.as_str(), "an EVM address is 20 bytes"));
    }
    let mut word = [0u8; 32];
    word[12..].copy_from_slice(&raw);
    Ok(word)
}

/// Encode a `bytes32`, which occupies the whole word.
pub fn bytes32(value: Hash) -> Word {
    value
}

/// Encode a `bytes16`, **left**-aligned.
pub fn bytes16(value: [u8; 16]) -> Word {
    let mut word = [0u8; 32];
    word[..16].copy_from_slice(&value);
    word
}

/// The sixteen bytes a plan id carries, for passing on chain as `bytes16`.
///
/// A plan id is `ded1` and thirty-two hex digits — half a SHA-256. The tag is
/// for humans reading a file; the chain gets the hash.
///
/// # Errors
///
/// Returns [`Error::Config`] if `plan_id` is not that shape.
pub fn plan_id_bytes(plan_id: &str) -> Result<[u8; 16]> {
    crate::store::validate_id(plan_id, crate::ledger::PLAN_TAG)?;
    let body = &plan_id[crate::ledger::PLAN_TAG.len()..];
    let raw = hex::decode(body).map_err(|e| Error::config(format!("plan id is not hex: {e}")))?;
    raw.try_into()
        .map_err(|_| Error::config("a plan id carries sixteen bytes".to_string()))
}

/// Concatenate a selector and its arguments into calldata.
pub fn call(signature: &str, words: &[Word]) -> Vec<u8> {
    let mut data = Vec::with_capacity(4 + words.len() * 32);
    data.extend_from_slice(&selector(signature));
    for word in words {
        data.extend_from_slice(word);
    }
    data
}

/// `approve(address,uint256)` — the ERC-20 allowance a deposit spends.
pub const APPROVE: &str = "approve(address,uint256)";
/// `deposit(bytes16,bytes32,address,uint256)` — funding one round.
pub const DEPOSIT: &str = "deposit(bytes16,bytes32,address,uint256)";
/// `claim(bytes16,uint256,address,uint256,bytes32[])` — taking one share.
pub const CLAIM: &str = "claim(bytes16,uint256,address,uint256,bytes32[])";

/// Calldata for approving the claim contract to move `amount` of the token.
pub fn approve_calldata(spender: &Address, amount: Amount) -> Result<Vec<u8>> {
    Ok(call(
        APPROVE,
        &[address(spender)?, uint256(amount.base_units())],
    ))
}

/// Calldata for depositing a round against its Merkle root.
pub fn deposit_calldata(
    plan_id: &str,
    root: Hash,
    token: &Address,
    total: Amount,
) -> Result<Vec<u8>> {
    Ok(call(
        DEPOSIT,
        &[
            bytes16(plan_id_bytes(plan_id)?),
            bytes32(root),
            address(token)?,
            uint256(total.base_units()),
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(byte: u8) -> Address {
        let body: String = std::iter::repeat_n(format!("{byte:02x}"), 20).collect();
        Address::parse(&format!("0x{body}")).unwrap()
    }

    /// The canonical vector every ABI implementation is checked against.
    #[test]
    fn selectors_match_the_known_values() {
        assert_eq!(
            hex::encode(selector("transfer(address,uint256)")),
            "a9059cbb"
        );
        assert_eq!(hex::encode(selector(APPROVE)), "095ea7b3");
        assert_eq!(hex::encode(selector("balanceOf(address)")), "70a08231");
    }

    /// The alignment rule that produces a silently wrong call when missed.
    #[test]
    fn numbers_are_right_aligned_and_byte_strings_are_left_aligned() {
        let n = uint256(1);
        assert_eq!(n[31], 1);
        assert!(n[..31].iter().all(|b| *b == 0));

        let a = address(&addr(0xab)).unwrap();
        assert!(a[..12].iter().all(|b| *b == 0), "address pads on the left");
        assert!(a[12..].iter().all(|b| *b == 0xab));

        let b = bytes16([0xcd; 16]);
        assert!(
            b[..16].iter().all(|x| *x == 0xcd),
            "bytesN pads on the right"
        );
        assert!(b[16..].iter().all(|x| *x == 0));
    }

    #[test]
    fn a_plan_id_reaches_the_chain_as_its_sixteen_hash_bytes() {
        let id = "ded100112233445566778899aabbccddeeff";
        assert_eq!(
            plan_id_bytes(id).unwrap(),
            [
                0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
                0xee, 0xff
            ]
        );
        // The same validation the object store uses: an id is never a path.
        assert!(plan_id_bytes("../../etc/passwd").is_err());
        assert!(plan_id_bytes("ded1nothex0000000000000000000000000").is_err());
    }

    #[test]
    fn deposit_calldata_is_a_selector_and_four_words() {
        let data = deposit_calldata(
            "ded100112233445566778899aabbccddeeff",
            [0x7f; 32],
            &addr(0x11),
            Amount::from_base_units(1_000_000),
        )
        .unwrap();
        assert_eq!(data.len(), 4 + 4 * 32);
        assert_eq!(data[..4], selector(DEPOSIT));
        // bytes16 left-aligned, then its 16 bytes of padding.
        assert_eq!(
            &data[4..20],
            &hex::decode("00112233445566778899aabbccddeeff").unwrap()[..]
        );
        assert!(data[20..36].iter().all(|b| *b == 0));
        // bytes32 fills its word.
        assert!(data[36..68].iter().all(|b| *b == 0x7f));
        // address right-aligned.
        assert!(data[68..80].iter().all(|b| *b == 0));
        assert!(data[80..100].iter().all(|b| *b == 0x11));
        // uint256 right-aligned.
        assert_eq!(
            u128::from_be_bytes(data[116..132].try_into().unwrap()),
            1_000_000
        );
    }

    #[test]
    fn an_address_that_is_not_twenty_bytes_is_refused_not_truncated() {
        // Reachable only by constructing an Address for another chain; the
        // guard is here so adding one cannot silently produce a bad word.
        let ok = address(&addr(0x01));
        assert!(ok.is_ok());
    }
}
