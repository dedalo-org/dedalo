//! Destination addresses.
//!
//! A payout that reaches the wrong address is not recoverable. There is no
//! support desk, no reversal, and no one to appeal to — so an address is
//! checked before it is ever written into a plan, not when a transaction
//! fails.
//!
//! Two properties matter here, and both have bitten this codebase:
//!
//! 1. **Comparison must be case-insensitive.** [EIP-55] encodes a checksum in
//!    the capitalisation of the hex digits, so the same account is routinely
//!    written two ways. Comparing the strings byte-for-byte made one
//!    contributor with two spellings of one address receive two transfers,
//!    breaking the "one wallet, one transfer" guarantee.
//! 2. **A mistyped address must be rejected.** The EIP-55 checksum catches
//!    essentially every single-character slip. An address that carries one is
//!    verified against it.
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
    checksummed: String,
}

impl Address {
    /// Validate and normalise an address.
    ///
    /// Accepts an all-lowercase or all-uppercase body, which carries no
    /// checksum, and verifies a mixed-case body against EIP-55.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Address`] when the string is not `0x` followed by 40
    /// hex digits, or when a mixed-case body fails its checksum — which is
    /// what a typo looks like.
    pub fn parse(value: &str) -> Result<Self> {
        let trimmed = value.trim();
        let body = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
            .ok_or_else(|| Error::address(value, "must start with `0x`"))?;

        if body.len() != BODY_LEN {
            return Err(Error::address(
                value,
                format!(
                    "expected {BODY_LEN} hex digits after `0x`, found {}",
                    body.len()
                ),
            ));
        }
        if let Some(bad) = body.chars().find(|c| !c.is_ascii_hexdigit()) {
            return Err(Error::address(value, format!("`{bad}` is not a hex digit")));
        }

        let lower = body.to_ascii_lowercase();
        let checksummed = format!("0x{}", checksum_body(&lower));

        // An all-one-case body carries no checksum to verify against; a mixed
        // one does, and a mismatch is the signature of a mistyped character.
        let uniform = body.chars().all(|c| !c.is_ascii_alphabetic())
            || body
                .chars()
                .filter(|c| c.is_ascii_alphabetic())
                .all(|c| c.is_ascii_lowercase())
            || body
                .chars()
                .filter(|c| c.is_ascii_alphabetic())
                .all(|c| c.is_ascii_uppercase());
        if !uniform && !checksummed.ends_with(body) {
            return Err(Error::address(
                value,
                format!("EIP-55 checksum does not match; did you mean {checksummed}?"),
            ));
        }

        Ok(Self { checksummed })
    }

    /// The checksummed form: what a plan records and a transaction carries.
    pub fn as_str(&self) -> &str {
        &self.checksummed
    }

    /// Lowercase form, used for comparison and as a map key.
    pub fn key(&self) -> String {
        self.checksummed.to_ascii_lowercase()
    }

    /// Whether this is the all-zero address.
    ///
    /// Valid to write down — the config template ships it as a placeholder —
    /// and never valid to send to, because nothing can be recovered from it.
    pub fn is_zero(&self) -> bool {
        self.key() == ZERO_ADDRESS
    }
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
        f.write_str(&self.checksummed)
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
    fn the_zero_address_parses_but_is_flagged() {
        let zero = Address::parse(ZERO_ADDRESS).unwrap();
        assert!(zero.is_zero());
        assert!(!Address::parse(EIP55[0]).unwrap().is_zero());
    }
}
