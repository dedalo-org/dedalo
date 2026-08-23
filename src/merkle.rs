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
//! leaf  = keccak256( keccak256( abi.encode(uint256 index, address account, uint256 amount) ) )
//! node  = keccak256( min(a, b) ‖ max(a, b) )
//! ```
//!
//! Three choices, each for a reason:
//!
//! - **Leaves are hashed twice.** A single hash lets a 64-byte leaf be
//!   presented as an internal node, which is the classic second-preimage
//!   attack on Merkle trees. Double hashing makes leaf and node preimages
//!   different lengths, so no leaf can ever masquerade as a node.
//! - **Pairs are sorted before hashing.** The proof then needs no direction
//!   bits, and the verifier is OpenZeppelin's `MerkleProof`, which is the most
//!   reviewed implementation available. Matching it is worth more than any
//!   cleverness of ours.
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

use sha3::{Digest, Keccak256};

use crate::error::{Error, Result};
use crate::money::Amount;
use crate::payout::PayoutPlan;
use crate::wallet::Address;

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
    /// `abi.encode(uint256, address, uint256)`: three 32-byte words.
    ///
    /// Addresses are right-aligned in their word, which is how the ABI encodes
    /// every type narrower than a word.
    fn abi_encoded(&self) -> Result<[u8; 96]> {
        let mut out = [0u8; 96];
        out[24..32].copy_from_slice(&self.index.to_be_bytes());
        out[44..64].copy_from_slice(&evm_address_bytes(&self.account)?);
        let amount = self.amount.base_units();
        out[80..96].copy_from_slice(&amount.to_be_bytes());
        Ok(out)
    }

    /// The double-hashed leaf this claim contributes to the tree.
    pub fn leaf(&self) -> Result<Hash> {
        let once = keccak(&self.abi_encoded()?);
        Ok(keccak(&once))
    }
}

/// The 20 raw bytes of an EVM address.
///
/// # Errors
///
/// Returns [`Error::Address`] if the address is not an EVM one. A non-EVM
/// address in a plan destined for an EVM claim contract is a configuration
/// mistake, and silently truncating it would send funds nowhere.
fn evm_address_bytes(address: &Address) -> Result<[u8; 20]> {
    let hex_body = address
        .as_str()
        .strip_prefix("0x")
        .ok_or_else(|| Error::address(address.as_str(), "not an EVM address"))?;
    let raw = hex::decode(hex_body)
        .map_err(|e| Error::address(address.as_str(), format!("not hex: {e}")))?;
    raw.try_into()
        .map_err(|_| Error::address(address.as_str(), "an EVM address is 20 bytes"))
}

fn keccak(bytes: &[u8]) -> Hash {
    let mut out = [0u8; 32];
    out.copy_from_slice(&Keccak256::digest(bytes));
    out
}

/// `keccak256(min(a, b) ‖ max(a, b))` — OpenZeppelin's `_hashPair`.
fn hash_pair(a: Hash, b: Hash) -> Hash {
    let mut buf = [0u8; 64];
    let (first, second) = if a <= b { (a, b) } else { (b, a) };
    buf[..32].copy_from_slice(&first);
    buf[32..].copy_from_slice(&second);
    keccak(&buf)
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
            let mut pairs = below.chunks_exact(2);
            for pair in &mut pairs {
                next.push(hash_pair(pair[0], pair[1]));
            }
            // Promoted, never hashed with itself: `hash_pair(x, x)` would let
            // a proof for `x` be replayed as a proof for the node above it.
            if let [odd] = pairs.remainder() {
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
            let sibling = if position % 2 == 0 {
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
        let body: String = std::iter::repeat_n(format!("{byte:02x}"), 20).collect();
        Address::parse(&format!("0x{body}")).unwrap()
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
        let encoded = claim.abi_encoded().unwrap();
        assert_eq!(encoded.len(), 96);
        // index 1, right-aligned in the first word.
        assert_eq!(encoded[31], 1);
        assert!(encoded[..31].iter().all(|b| *b == 0));
        // address, right-aligned in the second: 12 zero bytes then 20 of 0xaa.
        assert!(encoded[32..44].iter().all(|b| *b == 0));
        assert!(encoded[44..64].iter().all(|b| *b == 0xaa));
        // amount 255, right-aligned in the third.
        assert_eq!(encoded[95], 255);

        let expected = keccak(&keccak(&encoded));
        assert_eq!(claim.leaf().unwrap(), expected);
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

    #[test]
    fn the_total_is_what_a_deposit_must_cover() {
        let tree = ClaimTree::new(claims(4)).unwrap();
        assert_eq!(
            tree.total().unwrap(),
            Amount::from_base_units(1 + 2 + 3 + 4)
        );
    }
}
