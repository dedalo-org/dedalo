//! The Merkle tree a claim contract verifies proofs against.
//!
//! [`docs/settlement-architecture.md`] chose a pull model: a round is
//! deposited once against a root, and each contributor claims their own share
//! by presenting a proof. This module builds that root, offline and
//! deterministically, from the same [`PayoutPlan`] a reviewer already read.
//!
//! # The encoding, precisely
//!
//! The contract and this module must agree byte for byte or every proof
//! fails, so the scheme is stated here rather than left to be inferred:
//!
//! ```text
//! leaf  = sha256( sha256( index:u64le ‖ account:[u8;32] ‖ amount:u64le ) )
//! node  = sha256( min(a, b) ‖ max(a, b) )
//! ```
//!
//! sha256 because it is what Solana hashes with, little-endian because that is
//! what Borsh and the account model use, and packed rather than word-aligned
//! because thirty-two byte words are an EVM idea and padding a Solana leaf
//! would cost the program bytes to reproduce.
//!
//! **Amounts are `u64` here, and that is a constraint rather than a choice.**
//! An SPL token balance is a `u64`, so a round that does not fit one cannot be
//! settled at all. [`Claim::leaf`] refuses such a claim rather than truncating
//! it, because a truncated amount is a leaf that verifies against a proof and
//! pays the wrong number.
//!
//! Four choices, each for a reason:
//!
//! - **Leaves are hashed twice.** A single hash lets a 64-byte leaf be
//!   presented as an internal node, which is the classic second-preimage
//!   attack on Merkle trees. Double hashing makes leaf and node preimages
//!   different lengths, so no leaf can ever masquerade as a node.
//! - **Pairs are sorted before hashing.** The proof then needs no direction
//!   bits, and the shape matches the sorted-pair verifier that every Merkle
//!   distributor on Solana uses. Matching the thing reviewers already know how
//!   to read is worth more than any cleverness of ours.
//! - **The index is inside the leaf.** Two contributors with equal amounts
//!   would otherwise produce identical leaves, and one proof would claim both
//!   shares. The index also pins the plan's item order into the root, which
//!   `PayoutPlan::compute_id` already commits to.
//!
//! An odd node at the end of a level is promoted to the next level unchanged,
//! never duplicated: hashing a node with itself lets a proof be replayed one
//! level up.
//!
//! # Status
//!
//! The root is computed correctly and tested here. The contract that would
//! verify it is unaudited and undeployed — see
//! [`docs/settlement-architecture.md`] for the five things that must exist
//! before any of this moves real money.
//!
//! [`docs/settlement-architecture.md`]: https://github.com/dedalo-org/dedalo/blob/main/docs/settlement-architecture.md

use sha2::{Digest, Sha256};

use crate::chain::wallet::Address;
use crate::error::{Error, Result};
use crate::money::Amount;
use crate::payout::PayoutPlan;

/// A 32-byte hash, as the chain sees it.
pub type Hash = [u8; 32];

/// One payable share, in the form the contract checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    /// Position in the plan's payable items. Part of the leaf, so two equal
    /// amounts are still two distinct claims.
    pub index: u64,
    /// Who may claim it.
    pub account: Address,
    /// How much, in the asset's base units.
    pub amount: Amount,
}

impl Claim {
    /// The double-hashed leaf this claim contributes to the tree.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Address`] if the account is not a Solana address, and
    /// [`Error::Config`] if the amount does not fit the `u64` an SPL token
    /// balance is. The second is not a formatting problem: a round that cannot
    /// be represented cannot be settled, and finding that out here is better
    /// than finding it out from a program.
    pub fn leaf(&self) -> Result<Hash> {
        let units = self.amount.base_units();
        let amount = u64::try_from(units).map_err(|_| {
            Error::config(format!(
                "{units} base units does not fit a u64, and an SPL token balance is a u64 — \
                 this round cannot be settled as it stands"
            ))
        })?;
        Ok(leaf_of(self.index, self.account.pubkey_bytes()?, amount))
    }
}

/// The leaf encoding itself, over the bytes a chain actually holds.
///
/// Separate from [`Claim`] because this is the primitive both halves need and
/// they hold an address differently. Off chain an address is base58, because
/// that is what a plan records and a person reads; on chain it is thirty-two
/// bytes, and a program that carried the string form would spend compute and
/// account space turning one into the other on every call.
///
/// ```text
/// leaf = sha256( sha256( index:u64le ‖ account:[u8;32] ‖ amount:u64le ) )
/// ```
pub fn leaf_of(index: u64, account: [u8; 32], amount: u64) -> Hash {
    // Packed, little-endian: 8 + 32 + 8. No padding, because the alignment
    // rules that made the EVM pad to thirty-two byte words do not exist here
    // and every byte is one the program has to hash back.
    let mut encoded = [0u8; 48];
    encoded[..8].copy_from_slice(&index.to_le_bytes());
    encoded[8..40].copy_from_slice(&account);
    encoded[40..48].copy_from_slice(&amount.to_le_bytes());

    let once = sha256(&encoded);
    sha256(&once)
}

fn sha256(bytes: &[u8]) -> Hash {
    let mut out = [0u8; 32];
    out.copy_from_slice(&Sha256::digest(bytes));
    out
}

/// `sha256(min(a, b) ‖ max(a, b))` — the sorted-pair node every Solana Merkle
/// distributor uses, so a proof built here verifies against a reader's
/// expectations rather than against ours alone.
fn hash_pair(a: Hash, b: Hash) -> Hash {
    let mut buf = [0u8; 64];
    let (first, second) = if a <= b { (a, b) } else { (b, a) };
    buf[..32].copy_from_slice(&first);
    buf[32..].copy_from_slice(&second);
    sha256(&buf)
}

/// The tree over one round's claims.
#[derive(Debug, Clone)]
pub struct ClaimTree {
    claims: Vec<Claim>,
    /// Level 0 is the leaves; the last level is the single root.
    levels: Vec<Vec<Hash>>,
}

impl ClaimTree {
    /// Build the tree for a plan's payable items, in plan order.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] if the plan pays nobody — an empty tree has
    /// no root, and a deposit against no root is money with no way out.
    pub fn from_plan(plan: &PayoutPlan) -> Result<Self> {
        let claims = plan
            .payable_items()
            .enumerate()
            .map(|(index, item)| Claim {
                index: index as u64,
                account: item.wallet.clone(),
                amount: item.amount,
            })
            .collect::<Vec<_>>();
        Self::new(claims)
    }

    /// Build the tree from claims directly.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] if `claims` is empty.
    pub fn new(claims: Vec<Claim>) -> Result<Self> {
        if claims.is_empty() {
            return Err(Error::config(
                "a claim tree needs at least one payable item: a deposit \
                 against no root cannot be claimed by anyone",
            ));
        }
        let leaves = claims.iter().map(Claim::leaf).collect::<Result<Vec<_>>>()?;

        let mut levels = vec![leaves];
        while levels.last().expect("levels is never empty").len() > 1 {
            let below = levels.last().expect("levels is never empty");
            let mut next = Vec::with_capacity(below.len().div_ceil(2));
            let (pairs, remainder) = below.as_chunks::<2>();
            for [left, right] in pairs {
                next.push(hash_pair(*left, *right));
            }
            // Promoted, never hashed with itself: `hash_pair(x, x)` would let
            // a proof for `x` be replayed as a proof for the node above it.
            if let [odd] = remainder {
                next.push(*odd);
            }
            levels.push(next);
        }

        Ok(Self { claims, levels })
    }

    /// The root a deposit commits to.
    pub fn root(&self) -> Hash {
        self.levels
            .last()
            .and_then(|level| level.first())
            .copied()
            .expect("a non-empty tree has a root")
    }

    /// The root as `0x`-prefixed hex, which is how it appears in a plan and in
    /// a transaction.
    pub fn root_hex(&self) -> String {
        format!("0x{}", hex::encode(self.root()))
    }

    /// The claims, in the order their indices name.
    pub fn claims(&self) -> &[Claim] {
        &self.claims
    }

    /// Sum of every claim, which is what the deposit must cover.
    pub fn total(&self) -> Result<Amount> {
        self.claims
            .iter()
            .try_fold(Amount::ZERO, |acc, claim| acc.checked_add(claim.amount))
    }

    /// The sibling hashes proving `index` belongs to [`Self::root`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] if `index` is not a claim in this tree.
    pub fn proof(&self, index: usize) -> Result<Vec<Hash>> {
        if index >= self.claims.len() {
            return Err(Error::config(format!(
                "no claim at index {index}: the tree has {}",
                self.claims.len()
            )));
        }
        let mut proof = Vec::new();
        let mut position = index;
        for level in &self.levels {
            if level.len() == 1 {
                break;
            }
            // An odd node was promoted, so at this level it has no sibling.
            let sibling = if position.is_multiple_of(2) {
                position + 1
            } else {
                position - 1
            };
            if let Some(hash) = level.get(sibling) {
                proof.push(*hash);
            }
            position /= 2;
        }
        Ok(proof)
    }

    /// Check a proof the way the contract does.
    ///
    /// Present so the property tests can assert that every claim the tree
    /// produces verifies, and that nothing else does.
    pub fn verify(root: Hash, leaf: Hash, proof: &[Hash]) -> bool {
        let computed = proof
            .iter()
            .fold(leaf, |acc, sibling| hash_pair(acc, *sibling));
        computed == root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(byte: u8) -> Address {
        Address::from_pubkey_bytes([byte; 32])
    }

    fn claims(count: u64) -> Vec<Claim> {
        (0..count)
            .map(|index| Claim {
                index,
                account: address(index as u8 + 1),
                amount: Amount::from_base_units(u128::from(index) + 1),
            })
            .collect()
    }

    #[test]
    fn a_leaf_is_three_abi_words_hashed_twice() {
        let claim = Claim {
            index: 1,
            account: address(0xaa),
            amount: Amount::from_base_units(255),
        };
        // Rebuilt here rather than read back from the encoder, so the test
        // asserts the layout instead of agreeing with it. Packed, no padding.
        let mut expected_encoding = [0u8; 48];
        expected_encoding[0] = 1; // index, u64 little-endian
        expected_encoding[8..40].copy_from_slice(&[0xaa; 32]); // account
        expected_encoding[40] = 255; // amount, u64 little-endian

        let expected = sha256(&sha256(&expected_encoding));
        assert_eq!(claim.leaf().unwrap(), expected);
        assert_eq!(
            leaf_of(1, [0xaa; 32], 255),
            expected,
            "the primitive and the wrapper must agree"
        );
    }

    #[test]
    fn every_claim_proves_against_the_root() {
        for count in 1..=17u64 {
            let tree = ClaimTree::new(claims(count)).unwrap();
            let root = tree.root();
            for (index, claim) in tree.claims().iter().enumerate() {
                let proof = tree.proof(index).unwrap();
                assert!(
                    ClaimTree::verify(root, claim.leaf().unwrap(), &proof),
                    "claim {index} of {count} did not verify"
                );
            }
        }
    }

    #[test]
    fn a_claim_that_is_not_in_the_tree_does_not_verify() {
        let tree = ClaimTree::new(claims(5)).unwrap();
        let proof = tree.proof(0).unwrap();

        // Same proof, different amount: the classic attempt.
        let inflated = Claim {
            amount: Amount::from_base_units(1_000_000),
            ..tree.claims()[0].clone()
        };
        assert!(!ClaimTree::verify(
            tree.root(),
            inflated.leaf().unwrap(),
            &proof
        ));

        // Same amount, different account.
        let stolen = Claim {
            account: address(0xff),
            ..tree.claims()[0].clone()
        };
        assert!(!ClaimTree::verify(
            tree.root(),
            stolen.leaf().unwrap(),
            &proof
        ));

        // Right claim, someone else's proof.
        let other = tree.proof(1).unwrap();
        assert!(!ClaimTree::verify(
            tree.root(),
            tree.claims()[0].leaf().unwrap(),
            &other
        ));
    }

    /// Two contributors owed the same amount must be two claims. Without the
    /// index in the leaf they would share one, and one proof would take both.
    #[test]
    fn equal_amounts_are_still_distinct_claims() {
        let same = vec![
            Claim {
                index: 0,
                account: address(1),
                amount: Amount::from_base_units(500),
            },
            Claim {
                index: 1,
                account: address(1),
                amount: Amount::from_base_units(500),
            },
        ];
        let tree = ClaimTree::new(same).unwrap();
        assert_ne!(
            tree.claims()[0].leaf().unwrap(),
            tree.claims()[1].leaf().unwrap()
        );
    }

    #[test]
    fn the_root_is_stable_across_runs_and_moves_with_any_field() {
        let base = ClaimTree::new(claims(4)).unwrap();
        assert_eq!(base.root(), ClaimTree::new(claims(4)).unwrap().root());

        let mut changed = claims(4);
        changed[2].amount = Amount::from_base_units(999);
        assert_ne!(base.root(), ClaimTree::new(changed).unwrap().root());

        let mut reordered = claims(4);
        reordered.swap(0, 3);
        assert_ne!(
            base.root(),
            ClaimTree::new(reordered).unwrap().root(),
            "the index pins the order into the root"
        );
    }

    #[test]
    fn an_empty_tree_is_refused() {
        let error = ClaimTree::new(Vec::new()).unwrap_err().to_string();
        assert!(error.contains("at least one payable item"), "{error}");
    }

    /// **Domain: every tree size from one to sixty-four claims.**
    ///
    /// Proves, for that domain: every claim verifies against its own root, and
    /// no claim verifies against another claim's proof. Sixty-four covers
    /// every shape the promotion rule produces — six full levels, and every
    /// odd count that leaves a node without a sibling.
    ///
    /// Slow by construction, so `#[ignore]`; `ws-check` and the `verification`
    /// CI job run it with `--ignored`.
    #[test]
    #[ignore = "exhaustive: ~20s"]
    fn every_tree_size_up_to_sixty_four_proves_exactly_its_own_claims() {
        for size in 1..=64u64 {
            let claims: Vec<Claim> = (0..size)
                .map(|index| Claim {
                    index,
                    account: Address::from_pubkey_bytes([index as u8 + 1; 32]),
                    amount: Amount::from_base_units(u128::from(index) + 1),
                })
                .collect();
            let tree = ClaimTree::new(claims).unwrap();
            let root = tree.root();

            for (index, claim) in tree.claims().iter().enumerate() {
                let proof = tree.proof(index).unwrap();
                assert!(
                    ClaimTree::verify(root, claim.leaf().unwrap(), &proof),
                    "size {size}: claim {index} did not verify against its own root"
                );
                for (other, other_claim) in tree.claims().iter().enumerate() {
                    if other == index {
                        continue;
                    }
                    assert!(
                        !ClaimTree::verify(root, other_claim.leaf().unwrap(), &proof),
                        "size {size}: claim {other} verified with claim {index}'s proof"
                    );
                }
            }
        }
    }

    /// The leaf encoding, pinned.
    ///
    /// These values began as the cross-check against a second implementation
    /// in Solidity. That implementation is gone, and so is the chain it was
    /// for — but the reason to pin them outlived both: the encoding is what a
    /// deployed program will verify proofs against, and changing it silently
    /// would invalidate every round already deposited.
    ///
    /// **They were last changed deliberately**, when the leaf moved from
    /// `keccak256(abi.encode(...))` to `sha256` over packed little-endian
    /// fields, because the chain became Solana. Nothing had been deposited
    /// anywhere, so nothing was invalidated. That is the only circumstance in
    /// which this may move: a change here is allowed, it has to be deliberate,
    /// and the commit has to say why.
    #[test]
    fn the_leaf_encoding_has_not_moved() {
        let claims: Vec<Claim> = [
            ("So11111111111111111111111111111111111111112", 1_000u128),
            ("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA", 2_500),
            ("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU", 400),
            ("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL", 100),
            ("SysvarRent111111111111111111111111111111111", 7),
        ]
        .iter()
        .enumerate()
        .map(|(index, (account, amount))| Claim {
            index: index as u64,
            account: Address::parse(account).unwrap(),
            amount: Amount::from_base_units(*amount),
        })
        .collect();

        let tree = ClaimTree::new(claims).unwrap();
        assert_eq!(
            tree.root_hex(),
            "0x9900d807d47b7a32f9f4c07be1f02a90dbc0c68d1aa8bfb8944e27d7d445b16b"
        );
        assert_eq!(tree.total().unwrap(), Amount::from_base_units(4_007));

        // Five claims, so the last one sits on a promoted node and its proof is
        // one sibling rather than three.
        let short = tree.proof(4).unwrap();
        assert_eq!(short.len(), 1);
        assert_eq!(
            format!("0x{}", hex::encode(short[0])),
            "0xc6b1add2bcfc4ceaa0b003c378468f5b510353c41f893e4b15642a2d127b8849"
        );
    }

    #[test]
    fn the_total_is_what_a_deposit_must_cover() {
        let tree = ClaimTree::new(claims(4)).unwrap();
        assert_eq!(
            tree.total().unwrap(),
            Amount::from_base_units(1 + 2 + 3 + 4)
        );
    }
}
