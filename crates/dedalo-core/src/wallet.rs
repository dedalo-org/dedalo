//! Destination addresses.
//!
//! A payout that reaches the wrong address is not recoverable. There is no
//! support desk, no reversal, and no one to appeal to — so an address is
//! checked before it is ever written into a plan, not when a transaction
//! fails.
//!
//! # Adding a chain
//!
//! An address is a [`Address`] plus the [`AddressKind`] that says how to read
//! it. Today the only kind is [`AddressKind::Evm`]; a second one is four
//! things and no redesign:
//!
//! 1. a variant on [`AddressKind`];
//! 2. its name in [`AddressKind::for_chain`], so a config that names that
//!    chain expects that format;
//! 3. a branch in [`AddressKind::parse`] that validates and canonicalises;
//! 4. a branch in [`AddressKind::comparison_key`], which decides when two
//!    spellings are the same account.
//!
//! It is an enum rather than a trait deliberately. A trait with one
//! implementation is indirection with no payer; an enum with one variant says
//! exactly as much as is currently true, and the compiler lists every place
//! that needs a second one.
//!
//! # Two properties that have already bitten this codebase
//!
//! 1. **Comparison must follow the chain's rules.** [EIP-55] encodes a
//!    checksum in the capitalisation of the hex digits, so one EVM account is
//!    routinely written two ways. Comparing the strings byte-for-byte made a
//!    contributor with two spellings receive two transfers.
//! 2. **A mistyped address must be rejected.** The EIP-55 checksum catches
//!    essentially every single-character slip.
//!
//! [EIP-55]: https://eips.ethereum.org/EIPS/eip-55

use std::fmt;

use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

use crate::error::{Error, Result};

/// Length of the hex body of an address, without the `0x`.
const BODY_LEN: usize = 40;

/// The all-zero address. A valid encoding that no one holds the key to, so
/// anything sent there is destroyed.
pub const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";

/// A validated payout destination.
///
/// Stored in its [EIP-55] checksummed form, which is what a plan records and
/// what a transaction should carry. Comparison and hashing use the lowercase
/// form, so two spellings of one account are one payee.
///
/// [EIP-55]: https://eips.ethereum.org/EIPS/eip-55
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct Address {
    #[serde(skip)]
    kind: AddressKind,
    canonical: String,
}

impl Address {
    /// Validate and normalise an address, inferring which chain it belongs to.
    ///
    /// Address formats are distinct enough to tell apart by shape, and a
    /// string that matches none of them is rejected rather than guessed at.
    /// Use [`Address::parse_as`] when the expected chain is already known —
    /// it gives a better error than "unrecognised".
    ///
    /// # Errors
    ///
    /// Returns [`Error::Address`] when no known format matches, or when the
    /// matching format rejects it — for an EVM address, that includes a
    /// mixed-case body whose EIP-55 checksum fails, which is what a typo
    /// looks like.
    pub fn parse(value: &str) -> Result<Self> {
        let trimmed = value.trim();
        for kind in AddressKind::ALL {
            if kind.looks_like(trimmed) {
                return kind.parse(trimmed);
            }
        }
        Err(Error::address(
            value,
            format!(
                "does not match any supported address format ({})",
                AddressKind::ALL
                    .iter()
                    .map(|k| k.description())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ))
    }

    /// Validate an address against one specific chain's format.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Address`] if the value is not valid for `kind`.
    pub fn parse_as(kind: AddressKind, value: &str) -> Result<Self> {
        kind.parse(value.trim())
    }

    /// Which chain family this address belongs to.
    pub fn kind(&self) -> AddressKind {
        self.kind
    }

    /// The canonical form: what a plan records and a transaction carries.
    ///
    /// For EVM this is the [EIP-55] checksummed spelling.
    ///
    /// [EIP-55]: https://eips.ethereum.org/EIPS/eip-55
    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    /// The form two addresses are compared by, following the chain's rules.
    pub fn key(&self) -> String {
        self.kind.comparison_key(&self.canonical)
    }

    /// Whether this is the chain's burn address — a valid encoding that
    /// nobody holds the key to, so anything sent there is destroyed.
    pub fn is_zero(&self) -> bool {
        self.kind.is_zero(&self.canonical)
    }
}

/// The address format a chain uses.
///
/// See the module docs for what adding a variant involves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum AddressKind {
    /// `0x` followed by 20 hex-encoded bytes, as used by every EVM chain.
    Evm,
}

impl AddressKind {
    /// Every format this build understands, in detection order.
    pub const ALL: &'static [AddressKind] = &[AddressKind::Evm];

    /// The format a named chain expects, if this build knows the chain.
    ///
    /// Used to cross-check a config: an address that is well-formed for the
    /// wrong chain is still an address the funds cannot reach.
    pub fn for_chain(chain: &str) -> Option<Self> {
        const EVM: &[&str] = &[
            "ethereum",
            "mainnet",
            "base",
            "base-sepolia",
            "sepolia",
            "optimism",
            "arbitrum",
            "arbitrum-one",
            "polygon",
            "gnosis",
            "celo",
            "linea",
            "scroll",
            "zksync",
            "avalanche",
            "bsc",
        ];
        let chain = chain.trim().to_ascii_lowercase();
        EVM.contains(&chain.as_str()).then_some(AddressKind::Evm)
    }

    /// Short human name, used in error messages.
    pub fn description(self) -> &'static str {
        match self {
            AddressKind::Evm => "EVM: 0x followed by 40 hex digits",
        }
    }

    /// A cheap shape test, used to pick a format before validating properly.
    fn looks_like(self, value: &str) -> bool {
        match self {
            AddressKind::Evm => value.starts_with("0x") || value.starts_with("0X"),
        }
    }

    /// Validate and canonicalise.
    fn parse(self, value: &str) -> Result<Address> {
        match self {
            AddressKind::Evm => parse_evm(value),
        }
    }

    /// How this chain decides two spellings are the same account.
    fn comparison_key(self, canonical: &str) -> String {
        match self {
            // EIP-55 puts a checksum in the capitalisation, so case carries no
            // identity: two spellings of one account must compare equal.
            AddressKind::Evm => canonical.to_ascii_lowercase(),
        }
    }

    fn is_zero(self, canonical: &str) -> bool {
        match self {
            AddressKind::Evm => canonical.eq_ignore_ascii_case(ZERO_ADDRESS),
        }
    }
}

/// Validate an EVM address and return it in EIP-55 form.
fn parse_evm(trimmed: &str) -> Result<Address> {
    let body = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .ok_or_else(|| Error::address(trimmed, "must start with `0x`"))?;

    if body.len() != BODY_LEN {
        return Err(Error::address(
            trimmed,
            format!(
                "expected {BODY_LEN} hex digits after `0x`, found {}",
                body.len()
            ),
        ));
    }
    if let Some(bad) = body.chars().find(|c| !c.is_ascii_hexdigit()) {
        return Err(Error::address(
            trimmed,
            format!("`{bad}` is not a hex digit"),
        ));
    }

    let lower = body.to_ascii_lowercase();
    let canonical = format!("0x{}", checksum_body(&lower));

    // An all-one-case body carries no checksum to verify against; a mixed one
    // does, and a mismatch is the signature of a mistyped character.
    let letters: Vec<char> = body.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    let uniform = letters.is_empty()
        || letters.iter().all(|c| c.is_ascii_lowercase())
        || letters.iter().all(|c| c.is_ascii_uppercase());
    if !uniform && !canonical.ends_with(body) {
        return Err(Error::address(
            trimmed,
            format!("EIP-55 checksum does not match; did you mean {canonical}?"),
        ));
    }

    Ok(Address {
        kind: AddressKind::Evm,
        canonical,
    })
}

impl PartialEq for Address {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for Address {}

impl std::hash::Hash for Address {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key().hash(state);
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical)
    }
}

impl std::str::FromStr for Address {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::parse(value)
    }
}

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Address::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// Apply the EIP-55 capitalisation to a lowercase hex body.
fn checksum_body(lower: &str) -> String {
    let hash = Keccak256::digest(lower.as_bytes());
    lower
        .chars()
        .enumerate()
        .map(|(index, c)| {
            if c.is_ascii_digit() {
                return c;
            }
            // Each hex digit is scored by its own nibble of the hash.
            let byte = hash[index / 2];
            let nibble = if index % 2 == 0 {
                byte >> 4
            } else {
                byte & 0x0f
            };
            if nibble >= 8 {
                c.to_ascii_uppercase()
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The vectors from EIP-55 itself.
    const EIP55: [&str; 4] = [
        "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
        "0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359",
        "0xdbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB",
        "0xD1220A0cf47c7B9Be7A2E6BA89F429762e7b9aDb",
    ];

    #[test]
    fn reproduces_the_eip55_vectors() {
        for expected in EIP55 {
            let lower = expected.to_ascii_lowercase();
            assert_eq!(Address::parse(&lower).unwrap().as_str(), expected);
            assert_eq!(Address::parse(expected).unwrap().as_str(), expected);
        }
    }

    #[test]
    fn a_mistyped_character_is_caught_by_the_checksum() {
        // One character changed in a checksummed address.
        let broken = "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAee";
        let error = Address::parse(broken).unwrap_err().to_string();
        assert!(error.contains("checksum"), "{error}");
    }

    #[test]
    fn one_account_written_two_ways_is_one_payee() {
        let mixed = Address::parse("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed").unwrap();
        let lower = Address::parse("0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed").unwrap();
        let upper = Address::parse("0X5AAEB6053F3E94C9B9A09F33669435E7EF1BEAED").unwrap();
        assert_eq!(mixed, lower);
        assert_eq!(mixed, upper);
        assert_eq!(mixed.key(), lower.key());
    }

    #[test]
    fn rejects_anything_that_is_not_an_address() {
        for bad in [
            "",
            "definitely not an address",
            "0xtreasury",
            "5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
            "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAe",
            "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAedd",
            "0xzzAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
        ] {
            assert!(Address::parse(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn a_chain_maps_to_the_format_it_expects() {
        for chain in [
            "base",
            "Base",
            " ethereum ",
            "arbitrum",
            "optimism",
            "polygon",
        ] {
            assert_eq!(
                AddressKind::for_chain(chain),
                Some(AddressKind::Evm),
                "{chain} should be an EVM chain"
            );
        }
        // A chain this build knows nothing about is not guessed at.
        for chain in ["solana", "bitcoin", "", "not-a-chain"] {
            assert_eq!(
                AddressKind::for_chain(chain),
                None,
                "{chain} should be unknown"
            );
        }
    }

    #[test]
    fn an_unrecognised_format_says_what_is_supported() {
        // A Solana-style base58 address: valid somewhere, not here.
        let error = Address::parse("9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM")
            .unwrap_err()
            .to_string();
        assert!(error.contains("does not match any supported"), "{error}");
        assert!(
            error.contains("EVM"),
            "the error should list what is supported: {error}"
        );
    }

    #[test]
    fn parsing_for_a_named_chain_gives_the_specific_error() {
        let error = Address::parse_as(AddressKind::Evm, "not-hex-at-all")
            .unwrap_err()
            .to_string();
        assert!(error.contains("must start with `0x`"), "{error}");
    }

    #[test]
    fn the_canonical_form_round_trips_through_json() {
        let address = Address::parse("0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed").unwrap();
        let json = serde_json::to_string(&address).unwrap();
        assert_eq!(json, "\"0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed\"");
        let back: Address = serde_json::from_str(&json).unwrap();
        assert_eq!(back, address);
        assert_eq!(back.kind(), AddressKind::Evm);
    }

    #[test]
    fn the_zero_address_parses_but_is_flagged() {
        let zero = Address::parse(ZERO_ADDRESS).unwrap();
        assert!(zero.is_zero());
        assert!(!Address::parse(EIP55[0]).unwrap().is_zero());
    }
}
