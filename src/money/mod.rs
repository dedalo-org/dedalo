//! Exact money handling.
//!
//! Every value is stored as an integer number of *base units* (wei, satoshi,
//! USDC micro-units, ...). Floating point never touches a balance: a payout
//! plan must be reproducible bit-for-bit by anyone auditing the ledger.

#[cfg(test)]
mod proofs;

pub mod treasury;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The token a project pays contributors in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    /// Display symbol, e.g. `USDC`.
    pub symbol: String,
    /// Number of decimal places the token uses on chain.
    pub decimals: u8,
    /// Chain identifier as used by the settlement backend, e.g. `base`.
    pub chain: String,
    /// Token contract address. `None` means the chain's native coin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract: Option<String>,
}

impl Asset {
    /// Build the native coin of a chain, which has no token contract.
    pub fn native(symbol: impl Into<String>, chain: impl Into<String>, decimals: u8) -> Self {
        Self {
            symbol: symbol.into(),
            decimals,
            chain: chain.into(),
            contract: None,
        }
    }

    /// Parse a human decimal string ("12.5") into base units for this asset.
    pub fn parse_amount(&self, value: &str) -> Result<Amount> {
        Amount::parse(value, self.decimals)
    }

    /// Render base units as a human decimal string, without trailing noise.
    pub fn format_amount(&self, amount: Amount) -> String {
        amount.to_decimal_string(self.decimals)
    }
}

/// An integer quantity of base units of some [`Asset`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Amount(#[serde(with = "u128_str")] u128);

impl Amount {
    /// Nothing. The identity for addition and the default share.
    pub const ZERO: Amount = Amount(0);

    /// Wrap a raw count of base units.
    pub const fn from_base_units(units: u128) -> Self {
        Amount(units)
    }

    /// The raw count of base units.
    pub const fn base_units(self) -> u128 {
        self.0
    }

    /// Whether this amount would move nothing.
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Add, failing loudly instead of wrapping.
    pub fn checked_add(self, other: Amount) -> Result<Amount> {
        self.0
            .checked_add(other.0)
            .map(Amount)
            .ok_or(Error::Overflow("amount addition"))
    }

    /// Subtract, failing rather than going negative.
    pub fn checked_sub(self, other: Amount) -> Result<Amount> {
        self.0
            .checked_sub(other.0)
            .map(Amount)
            .ok_or(Error::Overflow("amount subtraction"))
    }

    /// Take `bps` basis points (1 bps = 0.01%) of this amount, rounding down.
    pub fn bps(self, bps: u16) -> Result<Amount> {
        self.0
            .checked_mul(bps as u128)
            .map(|v| Amount(v / 10_000))
            .ok_or(Error::Overflow("basis point multiplication"))
    }

    /// Parse a decimal string with at most `decimals` fractional digits.
    pub fn parse(value: &str, decimals: u8) -> Result<Amount> {
        let raw = value.trim().replace('_', "");
        let err = || Error::Amount {
            value: value.to_string(),
            decimals,
        };
        if raw.is_empty() || raw.starts_with('-') {
            return Err(err());
        }
        let (int_part, frac_part) = match raw.split_once('.') {
            Some((i, f)) => (i, f),
            None => (raw.as_str(), ""),
        };
        // `.5` means what it looks like, so an empty integer part is allowed —
        // but only when there are digits after the point. Without this, `"."`
        // splits into two empty halves, the digit checks below pass
        // vacuously, and a string containing no digit at all parses as zero.
        if int_part.is_empty() && frac_part.is_empty() {
            return Err(err());
        }
        let int_part = if int_part.is_empty() { "0" } else { int_part };
        if !int_part.bytes().all(|b| b.is_ascii_digit())
            || !frac_part.bytes().all(|b| b.is_ascii_digit())
            || frac_part.len() > decimals as usize
        {
            return Err(err());
        }
        let mut digits = String::with_capacity(int_part.len() + decimals as usize);
        digits.push_str(int_part);
        digits.push_str(frac_part);
        for _ in frac_part.len()..decimals as usize {
            digits.push('0');
        }
        digits.parse::<u128>().map(Amount).map_err(|_| err())
    }

    /// Render as a decimal string, trimming trailing fractional zeros.
    pub fn to_decimal_string(self, decimals: u8) -> String {
        if decimals == 0 {
            return self.0.to_string();
        }
        let divisor = 10u128.pow(decimals as u32);
        let int_part = self.0 / divisor;
        let frac_part = self.0 % divisor;
        if frac_part == 0 {
            return int_part.to_string();
        }
        let frac = format!("{frac_part:0width$}", width = decimals as usize);
        format!("{int_part}.{}", frac.trim_end_matches('0'))
    }

    /// Split `self` across `weights` using the largest-remainder method.
    ///
    /// The result always sums back to exactly `self`, so no dust is ever
    /// created or destroyed. Ties are broken by index, keeping the split
    /// deterministic for a given input ordering.
    pub fn split_by_weights(self, weights: &[u128]) -> Result<Vec<Amount>> {
        let total_weight: u128 = weights
            .iter()
            .try_fold(0u128, |acc, w| acc.checked_add(*w))
            .ok_or(Error::Overflow("weight sum"))?;
        if weights.is_empty() || total_weight == 0 {
            return Ok(vec![Amount::ZERO; weights.len()]);
        }

        let mut shares = Vec::with_capacity(weights.len());
        let mut remainders = Vec::with_capacity(weights.len());
        let mut distributed: u128 = 0;

        for (index, weight) in weights.iter().enumerate() {
            let scaled = self
                .0
                .checked_mul(*weight)
                .ok_or(Error::Overflow("weighted share"))?;
            let share = scaled / total_weight;
            remainders.push((scaled % total_weight, index));
            distributed += share;
            shares.push(share);
        }

        // Hand out the leftover base units, biggest remainder first.
        let mut leftover = self.0 - distributed;
        remainders.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        for (_, index) in remainders {
            if leftover == 0 {
                break;
            }
            shares[index] += 1;
            leftover -= 1;
        }

        Ok(shares.into_iter().map(Amount).collect())
    }
}

impl std::fmt::Display for Amount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// `u128` does not round-trip through JSON numbers safely, so it is stored as
/// a string in every serialized artifact (ledger entries, plans, receipts).
mod u128_str {
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    pub fn serialize<S: Serializer>(value: &u128, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u128, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Str(String),
            Num(u64),
        }
        match Repr::deserialize(d)? {
            Repr::Str(s) => s.parse().map_err(D::Error::custom),
            Repr::Num(n) => Ok(n as u128),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats_decimals() {
        let usdc = Asset::native("USDC", "base", 6);
        assert_eq!(usdc.parse_amount("12.5").unwrap().base_units(), 12_500_000);
        assert_eq!(usdc.parse_amount("0.000001").unwrap().base_units(), 1);
        assert_eq!(usdc.parse_amount("7").unwrap().base_units(), 7_000_000);
        assert_eq!(usdc.format_amount(Amount(12_500_000)), "12.5");
        assert_eq!(usdc.format_amount(Amount(1)), "0.000001");
        assert_eq!(usdc.format_amount(Amount::ZERO), "0");
    }

    #[test]
    fn rejects_over_precise_or_negative_amounts() {
        assert!(Amount::parse("1.0000001", 6).is_err());
        assert!(Amount::parse("-1", 6).is_err());
        assert!(Amount::parse("abc", 6).is_err());
    }

    #[test]
    fn split_conserves_every_base_unit() {
        let total = Amount::from_base_units(1_000_000);
        let weights = [3u128, 3, 3];
        let shares = total.split_by_weights(&weights).unwrap();
        let sum: u128 = shares.iter().map(|s| s.base_units()).sum();
        assert_eq!(sum, total.base_units());
        // Largest-remainder gives the extra unit to the first tied entry.
        assert_eq!(shares[0].base_units(), 333_334);
        assert_eq!(shares[1].base_units(), 333_333);
    }

    #[test]
    fn split_with_no_weight_pays_nobody() {
        let shares = Amount::from_base_units(500)
            .split_by_weights(&[0, 0])
            .unwrap();
        assert!(shares.iter().all(|s| s.is_zero()));
    }

    #[test]
    fn basis_points_round_down() {
        let amount = Amount::from_base_units(10_001);
        assert_eq!(amount.bps(500).unwrap().base_units(), 500);
    }

    /// `.5` is a number people type, and it means what it looks like.
    ///
    /// The empty integer part has its own branch, and nothing reached it.
    #[test]
    fn an_amount_may_omit_the_leading_zero() {
        assert_eq!(Amount::parse(".5", 6).unwrap().base_units(), 500_000);
        assert_eq!(Amount::parse(".000001", 6).unwrap().base_units(), 1);
        // Still not a licence to omit everything.
        assert!(Amount::parse(".", 6).is_err());
        assert!(Amount::parse("", 6).is_err());
    }

    /// An amount larger than `u128` is refused, not wrapped.
    ///
    /// Every balance in the system is a `u128` of base units. A decimal string
    /// that does not fit has to fail at the boundary, because the alternative
    /// is a number that is silently not the one somebody typed.
    #[test]
    fn an_amount_that_does_not_fit_is_refused() {
        let too_big = "1".repeat(40);
        assert!(Amount::parse(&too_big, 6).is_err());
        // u128::MAX has 39 digits; one more than that with no decimals is out.
        assert!(Amount::parse("340282366920938463463374607431768211456", 0).is_err());
        // And the largest that does fit still parses.
        assert!(Amount::parse("340282366920938463463374607431768211455", 0).is_ok());
    }

    /// A weight big enough to overflow the intermediate product is an error,
    /// not a wrapped share.
    ///
    /// `split_by_weights` multiplies the total by each weight before dividing.
    /// That product is where an overflow would happen, and an overflow here
    /// would hand somebody an arbitrary amount — the single worst failure this
    /// function has. Nothing reached the branch that refuses it.
    #[test]
    fn a_weight_that_would_overflow_the_split_is_refused() {
        let everything = Amount::from_base_units(u128::MAX);
        let error = everything.split_by_weights(&[2, 1]).unwrap_err();
        assert!(
            matches!(error, Error::Overflow("weighted share")),
            "expected an overflow refusal, got {error:?}"
        );

        // The sum of the weights is where the other overflow lives.
        let error = everything
            .split_by_weights(&[u128::MAX, u128::MAX])
            .unwrap_err();
        assert!(
            matches!(error, Error::Overflow("weight sum")),
            "expected an overflow refusal, got {error:?}"
        );

        // A weight of one is the largest that cannot overflow, and works.
        let shares = everything.split_by_weights(&[1]).unwrap();
        assert_eq!(shares[0].base_units(), u128::MAX);
    }

    /// An amount serialises as a string and deserialises from either.
    ///
    /// Amounts are written as decimal strings because JSON numbers are doubles
    /// and lose precision above 2^53. Reading a bare number anyway is a
    /// deliberate leniency for hand-written config, and it had no test — so
    /// nothing said whether the lenient path produced the same value as the
    /// strict one.
    #[test]
    fn an_amount_round_trips_as_a_string_and_reads_a_number_too() {
        let amount = Amount::from_base_units(9_007_199_254_740_993);

        let json = serde_json::to_string(&amount).unwrap();
        assert_eq!(json, "\"9007199254740993\"", "amounts serialise as strings");
        assert_eq!(serde_json::from_str::<Amount>(&json).unwrap(), amount);

        // The lenient path: a JSON number, which is what somebody writes by
        // hand. Small enough to survive a double, which is the only reason it
        // is accepted at all.
        let from_number: Amount = serde_json::from_str("1000000").unwrap();
        assert_eq!(from_number.base_units(), 1_000_000);

        // Not a number, not a string of digits: refused rather than defaulted.
        assert!(serde_json::from_str::<Amount>("\"twelve\"").is_err());
    }
}
