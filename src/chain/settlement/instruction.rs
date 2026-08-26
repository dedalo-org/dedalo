//! Encoding the instructions a signer executes.
//!
//! A round is funded by people approving and executing two instructions from a
//! multisig. `dedalo propose` prints them with their data encoded, so a signer
//! compares them against a plan they can read rather than trusting a tool they
//! cannot.
//!
//! # Why this is encoded by hand, when the EVM version deliberately was not
//!
//! The module this replaces used a library, and said so at length: the ABI has
//! two alignment rules — `uintN` and `address` right-aligned, `bytesN` left —
//! and they are exactly the kind of detail a hand-rolled encoder gets right
//! until somebody edits it.
//!
//! **Borsh has no alignment rules.** Every integer is little-endian, packed,
//! in declaration order; a fixed array is its bytes; a vector is a `u32`
//! length followed by its elements. There is no padding to get wrong, so the
//! reason to depend on a library here has gone, and what is left is forty
//! lines a reviewer can check against the layout above without leaving the
//! file.
//!
//! # The two instructions
//!
//! They come from different programs, which is worth noticing:
//!
//! - **`Approve`** belongs to the SPL Token program and uses its own encoding:
//!   a one-byte tag, then the argument. It is not an Anchor instruction and
//!   does not carry a discriminator.
//! - **`deposit`** belongs to the claim program, which is Anchor, so its data
//!   starts with the eight-byte discriminator Anchor derives from the
//!   instruction's name.
//!
//! # What is an argument, and what is an account
//!
//! On the EVM, `deposit` took the token address as a parameter. On Solana the
//! token, the vault, the payer and the token program are all *accounts* on the
//! instruction rather than bytes in its data — so they do not appear here.
//! That is not an omission: the account list a signer must check is built by
//! [`super::proposal`], and putting an address in both places would let the
//! two disagree.
//!
//! The claim program is unwritten, unaudited and undeployed. See
//! [`docs/settlement-architecture.md`].
//!
//! [`docs/settlement-architecture.md`]: https://github.com/dedalo-org/dedalo/blob/main/docs/settlement-architecture.md

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::money::Amount;

/// SPL Token's instruction tag for `Approve`.
///
/// The token program dispatches on a single leading byte, and this is the
/// fifth variant of its instruction enum. Written as a named constant because
/// a bare `3` or `5` in the wrong place is an instruction that does something
/// else entirely and still encodes cleanly.
const SPL_TOKEN_APPROVE: u8 = 4;

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

/// Anchor's discriminator for an instruction: `sha256("global:<name>")[..8]`.
///
/// Anchor prefixes every instruction with this so a program can dispatch
/// without a tag byte it has to assign by hand. Deriving it rather than
/// hardcoding eight bytes means renaming an instruction here changes the
/// discriminator, which is what a program would also do.
pub fn discriminator(name: &str) -> [u8; 8] {
    let digest = Sha256::digest(format!("global:{name}").as_bytes());
    let mut out = [0u8; 8];
    out.copy_from_slice(&digest[..8]);
    out
}

/// An SPL token amount, which is a `u64` and not negotiable.
///
/// # Errors
///
/// Returns [`Error::Config`] if the amount does not fit. A round larger than a
/// token can represent cannot be settled, and truncating it here would produce
/// an instruction that executes and moves the wrong number.
fn token_amount(amount: Amount) -> Result<u64> {
    let units = amount.base_units();
    u64::try_from(units).map_err(|_| {
        Error::config(format!(
            "{units} base units does not fit a u64, and an SPL token balance is a u64 — \
             this round cannot be settled as it stands"
        ))
    })
}

/// Instruction data for delegating `amount` of the token to the vault.
///
/// SPL Token's `Approve`: the delegate and the source account are accounts on
/// the instruction, so the data is the tag and the amount and nothing else.
///
/// # Errors
///
/// Returns [`Error::Config`] if the amount does not fit a `u64`.
pub fn approve_data(amount: Amount) -> Result<Vec<u8>> {
    let mut data = Vec::with_capacity(9);
    data.push(SPL_TOKEN_APPROVE);
    data.extend_from_slice(&token_amount(amount)?.to_le_bytes());
    Ok(data)
}

/// Instruction data for depositing a round against its Merkle root.
///
/// Anchor layout: the discriminator, then the arguments in declaration order,
/// each little-endian and packed.
///
/// ```text
/// discriminator:[u8;8] ‖ plan_id:[u8;16] ‖ root:[u8;32] ‖ total:u64le
/// ```
///
/// # Errors
///
/// Returns [`Error::Config`] if the plan id is malformed or the total does not
/// fit a `u64`.
pub fn deposit_data(
    plan_id: &str,
    root: crate::chain::merkle::Hash,
    total: Amount,
) -> Result<Vec<u8>> {
    let mut data = Vec::with_capacity(8 + 16 + 32 + 8);
    data.extend_from_slice(&discriminator("deposit"));
    data.extend_from_slice(&plan_id_bytes(plan_id)?);
    data.extend_from_slice(&root);
    data.extend_from_slice(&token_amount(total)?.to_le_bytes());
    Ok(data)
}

/// Instruction data for taking one share.
///
/// Not built by this crate in the normal path — a contributor's wallet builds
/// it — but encoded here so the layout this project commits to lives in one
/// place, and so a change to it fails a test rather than silently producing
/// data nothing answers.
///
/// ```text
/// discriminator:[u8;8] ‖ index:u64le ‖ amount:u64le ‖ proof_len:u32le ‖ proof:[[u8;32]]
/// ```
///
/// # Errors
///
/// Returns [`Error::Config`] if the amount does not fit a `u64`.
pub fn claim_data(
    index: u64,
    amount: Amount,
    proof: &[crate::chain::merkle::Hash],
) -> Result<Vec<u8>> {
    let mut data = Vec::with_capacity(8 + 8 + 8 + 4 + proof.len() * 32);
    data.extend_from_slice(&discriminator("claim"));
    data.extend_from_slice(&index.to_le_bytes());
    data.extend_from_slice(&token_amount(amount)?.to_le_bytes());
    // Borsh writes a vector's length as a u32, and a proof longer than that is
    // not a thing any tree this crate builds can produce.
    let len = u32::try_from(proof.len())
        .map_err(|_| Error::config("a proof of that length is not encodable".to_string()))?;
    data.extend_from_slice(&len.to_le_bytes());
    for node in proof {
        data.extend_from_slice(node);
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_approve_instruction_is_the_token_program_layout() {
        let data = approve_data(Amount::from_base_units(1_000_000)).unwrap();
        assert_eq!(data.len(), 9, "a tag and a u64, and nothing else");
        assert_eq!(data[0], 4, "SPL Token dispatches Approve on a leading 4");
        assert_eq!(
            u64::from_le_bytes(data[1..9].try_into().unwrap()),
            1_000_000
        );
    }

    /// Anchor's discriminator is a published rule, so it is checked against
    /// the rule rather than against itself.
    #[test]
    fn the_discriminator_is_the_first_eight_bytes_of_the_namespaced_hash() {
        let expected = &Sha256::digest(b"global:deposit")[..8];
        assert_eq!(&discriminator("deposit"), expected);
        assert_ne!(
            discriminator("deposit"),
            discriminator("claim"),
            "two instructions must not dispatch to the same arm"
        );
    }

    #[test]
    fn deposit_data_is_packed_little_endian_in_declaration_order() {
        let data = deposit_data(
            "ded100112233445566778899aabbccddeeff",
            [0x7f; 32],
            Amount::from_base_units(1_000_000),
        )
        .unwrap();

        assert_eq!(data.len(), 8 + 16 + 32 + 8, "no padding anywhere");
        assert_eq!(&data[..8], &discriminator("deposit"));
        assert_eq!(
            &data[8..24],
            &hex::decode("00112233445566778899aabbccddeeff").unwrap()[..]
        );
        assert!(data[24..56].iter().all(|b| *b == 0x7f));
        assert_eq!(
            u64::from_le_bytes(data[56..64].try_into().unwrap()),
            1_000_000
        );
    }

    #[test]
    fn a_proof_is_a_u32_length_then_its_nodes() {
        let proof = [[1u8; 32], [2u8; 32]];
        let data = claim_data(7, Amount::from_base_units(5), &proof).unwrap();
        assert_eq!(data.len(), 8 + 8 + 8 + 4 + 64);
        assert_eq!(u64::from_le_bytes(data[8..16].try_into().unwrap()), 7);
        assert_eq!(u32::from_le_bytes(data[24..28].try_into().unwrap()), 2);
        assert!(data[28..60].iter().all(|b| *b == 1));
        assert!(data[60..92].iter().all(|b| *b == 2));
    }

    /// An SPL balance is a u64. A round that does not fit one cannot be
    /// settled, and finding that out here beats finding it out from a program.
    #[test]
    fn an_amount_too_large_for_a_token_is_refused_not_truncated() {
        let too_much = Amount::from_base_units(u128::from(u64::MAX) + 1);
        assert!(approve_data(too_much).is_err());
        assert!(deposit_data("ded100112233445566778899aabbccddeeff", [0; 32], too_much).is_err());
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
