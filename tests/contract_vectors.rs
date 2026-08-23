//! The vectors `contracts/test/DedaloClaim.t.sol` is pinned to.
//!
//! Two implementations of one encoding — `dedalo::merkle` in Rust and
//! `DedaloClaim._verify` in Solidity — are two chances to get it wrong. The
//! contract's test suite hardcodes the root and proofs below and checks that
//! they verify on chain; this file checks that Rust still produces them.
//!
//! Change the leaf encoding and both fail, which is the point: neither side
//! can drift without the other noticing.

use dedalo::merkle::{Claim, ClaimTree};
use dedalo::money::Amount;
use dedalo::wallet::Address;

/// Five claims, so the tree has an odd level. That exercises the promoted
/// node — the case an implementation that duplicates the last node gets wrong,
/// and the case where a proof is shorter than the others.
fn fixture() -> ClaimTree {
    let claims = [
        ("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed", 1_000u128),
        ("0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359", 2_500),
        ("0xdbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB", 400),
        ("0xD1220A0cf47c7B9Be7A2E6BA89F429762e7b9aDb", 100),
        ("0x9d33df7B2951b0086D40814475869BE3A485a146", 7),
    ]
    .iter()
    .enumerate()
    .map(|(index, (account, amount))| Claim {
        index: index as u64,
        account: Address::parse(account).unwrap(),
        amount: Amount::from_base_units(*amount),
    })
    .collect();
    ClaimTree::new(claims).unwrap()
}

fn hex_proof(tree: &ClaimTree, index: usize) -> Vec<String> {
    tree.proof(index)
        .unwrap()
        .iter()
        .map(|hash| format!("0x{}", hex::encode(hash)))
        .collect()
}

#[test]
fn the_root_the_contract_tests_are_pinned_to() {
    let tree = fixture();
    assert_eq!(
        tree.root_hex(),
        "0xffcf57755a292ee72605206f2e2fe131b222cb0ebd45c0844f68a187a384ec72",
        "the root moved: update contracts/test/DedaloClaim.t.sol to match, and \
         say in the commit message why the encoding changed"
    );
    assert_eq!(tree.total().unwrap(), Amount::from_base_units(4_007));
}

#[test]
fn the_proofs_the_contract_tests_are_pinned_to() {
    let tree = fixture();
    const SHARED_TOP: &str = "0x5e3abc22b830c6906076486238b2f41f3eb3b577a0d8b3d4c00f4358d9395679";

    assert_eq!(
        hex_proof(&tree, 0),
        [
            "0xfc5edd4a1fc1f4324bcbb5baa5314f08da758eb87d1f5e805bdb62888ced4103",
            "0xbb63fdb7acb073e28cfa14c1eddbc700ffdd8c83f028b7fc4c317be8f391e6a2",
            SHARED_TOP,
        ]
    );
    assert_eq!(
        hex_proof(&tree, 1),
        [
            "0xc285ac2f86e35db53e495dd5c9f5c14377d686735f1dd08928c36e2a51467166",
            "0xbb63fdb7acb073e28cfa14c1eddbc700ffdd8c83f028b7fc4c317be8f391e6a2",
            SHARED_TOP,
        ]
    );
    assert_eq!(
        hex_proof(&tree, 2),
        [
            "0xcfddde4f42b48651081968e095d417d12fc403218c0ef9175c71dafc7b89c845",
            "0x8744e01a4ec405c244f037e4056b784d96f955ade7b0b24b3cef6340db43268b",
            SHARED_TOP,
        ]
    );
    assert_eq!(
        hex_proof(&tree, 3),
        [
            "0x882827f712a338f3617721acd74cb7d8183be9edfe067bdc5cb8db60262e7954",
            "0x8744e01a4ec405c244f037e4056b784d96f955ade7b0b24b3cef6340db43268b",
            SHARED_TOP,
        ]
    );

    // The promoted node: one sibling, not three.
    assert_eq!(
        hex_proof(&tree, 4),
        ["0x3c10e56238056db4830e9497627d8c89435b42a0bc7142797badd5ec307a4af2"]
    );
}
