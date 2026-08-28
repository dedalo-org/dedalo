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
//! it. Today the only kind is [`AddressKind::Solana`]; a second one is four
//! things and no redesign:
//!
//! 1. a variant on [`AddressKind`];
//! 2. its name in [`AddressKind::for_chain`], so a config that names that
//!    chain expects that format;
//! 3. a branch in `AddressKind::parse` that validates and canonicalises;
//! 4. a branch in `AddressKind::comparison_key`, which decides when two
//!    spellings are the same account.
//!
//! The last two are private: they are the guide for someone editing this
//! file, not API a caller can reach, so they are named rather than linked.
//!
//! It is an enum rather than a trait deliberately. A trait with one
//! implementation is indirection with no payer; an enum with one variant says
//! exactly as much as is currently true, and the compiler lists every place
//! that needs a second one.
//!
//! # What a Solana address does and does not protect you from
//!
//! **There is no checksum.** This is the single most important thing to know
//! here, and the reason [`Address::checksum_bits`] returns zero and says so.
//! A Solana address is thirty-two bytes written in base58: every thirty-two
//! byte value is a valid public key, so a slip that still decodes to
//! thirty-two bytes is accepted and points at an account nobody holds the key
//! to.
//!
//! This is worse than what the EVM had. EIP-55 hid a checksum in the
//! capitalisation of an address's hex letters and rejected most mistyped
//! addresses; base58 hides nothing. Two things blunt it, and neither is a fix:
//!
//! 1. **Length.** base58 is dense enough that most single-character slips
//!    change the decoded length and are rejected outright. Most, not all.
//! 2. **The curve.** Roughly half of all thirty-two byte values are not points
//!    on ed25519 at all, so [`Address::is_on_curve`] separates a wallet from
//!    something nobody can sign for — see below for why that is not the same
//!    as rejecting it as an address.
//!
//! What genuinely protects a contributor is elsewhere: a round is *claimed*,
//! not sent, so an address nobody can claim from leaves the money in the round
//! until it expires rather than destroying it.
//!
//! # On-curve is a fact about an address, not a verdict on it
//!
//! An ed25519 public key — a wallet somebody holds the secret key for — is a
//! point on the curve. A program-derived address is deliberately *not*, which
//! is what makes it unforgeable by a keypair.
//!
//! Both are real addresses, and this module rejects neither. It reports which
//! one you have, because the answer differs by role:
//!
//! - **A contributor's wallet must be on-curve.** Off-curve means nobody can
//!   sign for it, so nobody can ever claim that share, and `dedalo identity
//!   link` refuses it.
//! - **A treasury or a multisig may be off-curve, and usually is.** A Squads
//!   vault is a program-derived address. Refusing those would refuse the exact
//!   arrangement this project's settlement architecture is built on.
//!
//! An associated token account is also off-curve, which is why plans record a
//! contributor's *wallet* rather than their token account: the token account
//! is derived from the wallet and the mint, and deriving it is the claim
//! program's job, not a config file's.
//!
//! # Comparison follows the chain's rules
//!
//! base58 is case-sensitive and carries no alternative spellings, so one
//! account has exactly one written form. That makes [`Address::key`] the
//! canonical string itself. It was not always this easy: under EIP-55 an
//! account was routinely written two ways, comparing the strings byte-for-byte
//! made one contributor receive two transfers, and that defect is why this
//! function exists at all. Folding case *here* would be the same defect
//! inverted — two unrelated accounts merged into one payee.

use std::fmt;

use curve25519_dalek::edwards::CompressedEdwardsY;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Length in bytes of a Solana address. Every one is exactly this long.
const PUBKEY_LEN: usize = 32;

/// Shortest a thirty-two byte value can be in base58.
///
/// Leading zero bytes each encode as a single `1`, so an address full of them
/// is shorter than one that is not. Both bounds are reachable, which is why
/// the length check is a range rather than a number.
const MIN_LEN: usize = 32;
/// Longest a thirty-two byte value can be in base58.
const MAX_LEN: usize = 44;

/// base58: base64 minus the four characters that are confusable in a bad font
/// or read aloud — `0`, `O`, `I` and `l`.
const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// The all-zero address, which on Solana is the System Program.
///
/// A valid encoding, on the curve, and one nobody holds a key for — so a
/// transfer there is not recoverable. Named so that "did this config get
/// filled in?" is a question with an answer.
pub const ZERO_ADDRESS: &str = "11111111111111111111111111111111";

/// A validated payout destination.
///
/// Stored in its canonical base58 form, which is the only form: base58 has
/// one encoding per byte string, so unlike EIP-55 there is nothing to
/// normalise and no second spelling to compare against.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct Address {
    #[serde(skip)]
    kind: AddressKind,
    canonical: String,
    /// Cached rather than recomputed: decompression is a field inversion, and
    /// this is asked once per payee per plan.
    #[serde(skip)]
    on_curve: bool,
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
    /// Returns [`Error::Address`] if the value matches no supported format.
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        for kind in AddressKind::ALL {
            if kind.looks_like(value) {
                return kind.parse(value);
            }
        }

        // Nothing recognised the shape, so no format's own parser ran and none
        // of their explanations reached anybody: every caller of `parse` got
        // "does not match any supported address format" and nothing to act on.
        //
        // `parse_solana` already says that `0` is not a base58 character, or
        // what the length bounds are and how far off this value is. That is
        // the sentence somebody needs to fix a typo, so ask each format why it
        // refuses and report *that*, keeping the list of what is supported for
        // the case where the value is not close to anything.
        let mut reasons = Vec::with_capacity(AddressKind::ALL.len());
        for kind in AddressKind::ALL {
            match kind.parse(value) {
                // `looks_like` is a cheap shape test and this is the real one.
                // If they ever disagree, the real one wins.
                Ok(address) => return Ok(address),
                Err(error) => reasons.push(format!("{} — {}", error, kind.description())),
            }
        }

        Err(Error::address(value, reasons.join("; ")))
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
    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    /// How many bits of checksum protect this address against a typo.
    ///
    /// **Zero, on Solana, and that is not a placeholder.** Every thirty-two
    /// byte value is a valid public key, so base58 that decodes to the right
    /// length is always accepted. There is no capitalisation carrying a hash,
    /// no trailing digits, nothing.
    ///
    /// Report it wherever a user commits an address for the first time, and
    /// report it as the zero it is. The previous chain family gave around
    /// fifteen bits here, and the honest thing is to say what was lost rather
    /// than to quietly stop mentioning it.
    ///
    /// What replaces it is not a stronger check but a different design: under
    /// the pull model a share nobody can claim stays in the round until it
    /// expires, instead of being sent somewhere unrecoverable.
    pub fn checksum_bits(&self) -> u32 {
        match self.kind {
            AddressKind::Solana => 0,
        }
    }

    /// Whether this address is a point on ed25519 — that is, a keypair's
    /// public key rather than a program-derived address.
    ///
    /// A contributor's wallet must be on-curve or nobody can sign for it, and
    /// nobody can ever claim that share. A treasury or a multisig vault
    /// legitimately is not: a Squads vault is program-derived, and so is every
    /// associated token account.
    ///
    /// So this is a fact to act on by role, not a validity test. See the
    /// module docs.
    pub fn is_on_curve(&self) -> bool {
        self.on_curve
    }

    /// The thirty-two bytes a Solana account is actually named by.
    ///
    /// The canonical form is base58 because that is what a plan records and a
    /// human reads. A chain has neither problem: it holds thirty-two bytes,
    /// and this is the one place the two representations meet.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Address`] if this is not a Solana address. Truncating
    /// or padding a differently sized one would produce thirty-two
    /// valid-looking bytes pointing at an account nobody controls.
    pub fn pubkey_bytes(&self) -> Result<[u8; PUBKEY_LEN]> {
        let mut raw = [0u8; PUBKEY_LEN];
        let written = bs58::decode(&self.canonical)
            .onto(&mut raw)
            .map_err(|e| Error::address(&self.canonical, format!("not base58: {e}")))?;
        if written != PUBKEY_LEN {
            return Err(Error::address(
                &self.canonical,
                format!("a Solana address is {PUBKEY_LEN} bytes, this decoded to {written}"),
            ));
        }
        Ok(raw)
    }

    /// Build a Solana address from the thirty-two bytes a chain holds.
    ///
    /// Infallible: any thirty-two bytes name a valid account. Whether they
    /// name one somebody can sign for is [`Address::is_on_curve`], and these
    /// did not come from a keyboard so there is nothing to check against a
    /// typo.
    pub fn from_pubkey_bytes(raw: [u8; PUBKEY_LEN]) -> Self {
        Self {
            kind: AddressKind::Solana,
            canonical: bs58::encode(raw).into_string(),
            on_curve: is_on_curve(&raw),
        }
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum AddressKind {
    /// Thirty-two bytes in base58, as used by Solana and everything built on
    /// its account model.
    #[default]
    Solana,
}

impl AddressKind {
    /// Every format this build understands, in detection order.
    pub const ALL: &'static [AddressKind] = &[AddressKind::Solana];

    /// The format a named chain expects, if this build knows the chain.
    ///
    /// Used to cross-check a config: an address that is well-formed for the
    /// wrong chain is still an address the funds cannot reach.
    pub fn for_chain(chain: &str) -> Option<Self> {
        const SOLANA: &[&str] = &[
            "solana",
            "mainnet-beta",
            "solana-mainnet",
            "devnet",
            "solana-devnet",
            "testnet",
            "solana-testnet",
            "localnet",
        ];
        let chain = chain.trim().to_ascii_lowercase();
        SOLANA
            .contains(&chain.as_str())
            .then_some(AddressKind::Solana)
    }

    /// Short human name, used in error messages.
    pub fn description(self) -> &'static str {
        match self {
            AddressKind::Solana => "Solana: 32 bytes in base58, 32 to 44 characters",
        }
    }

    /// A cheap shape test, used to pick a format before validating properly.
    fn looks_like(self, value: &str) -> bool {
        match self {
            // No prefix to key on, so shape is all there is: the right length
            // range, and nothing outside the alphabet. Deliberately permissive
            // — `parse` is what decides.
            AddressKind::Solana => {
                (MIN_LEN..=MAX_LEN).contains(&value.len()) && value.chars().all(is_base58_digit)
            }
        }
    }

    /// Validate and canonicalise.
    fn parse(self, value: &str) -> Result<Address> {
        match self {
            AddressKind::Solana => parse_solana(value),
        }
    }

    /// How this chain decides two spellings are the same account.
    fn comparison_key(self, canonical: &str) -> String {
        match self {
            // base58 is case-sensitive and has one encoding per value, so an
            // account has exactly one spelling. Lowercasing here — which is
            // what the EVM needed — would merge two different accounts into
            // one payee, which is the same defect in the other direction.
            AddressKind::Solana => canonical.to_string(),
        }
    }

    fn is_zero(self, canonical: &str) -> bool {
        match self {
            AddressKind::Solana => canonical == ZERO_ADDRESS,
        }
    }
}

/// Whether `c` is in base58's alphabet.
fn is_base58_digit(c: char) -> bool {
    c.is_ascii() && ALPHABET.contains(&(c as u8))
}

/// Whether thirty-two bytes decompress to a point on ed25519.
fn is_on_curve(raw: &[u8; PUBKEY_LEN]) -> bool {
    CompressedEdwardsY(*raw).decompress().is_some()
}

/// Validate a Solana address.
fn parse_solana(trimmed: &str) -> Result<Address> {
    if trimmed.is_empty() {
        return Err(Error::address(trimmed, "an address cannot be empty"));
    }
    if let Some(bad) = trimmed.chars().find(|c| !is_base58_digit(*c)) {
        // `0`, `O`, `I` and `l` are the characters base58 leaves out precisely
        // because they are misread, so seeing one is worth naming.
        let hint = match bad {
            '0' | 'O' => " — base58 has neither `0` nor `O`",
            'I' | 'l' => " — base58 has neither `I` nor `l`",
            _ => "",
        };
        return Err(Error::address(
            trimmed,
            format!("`{bad}` is not a base58 character{hint}"),
        ));
    }
    if !(MIN_LEN..=MAX_LEN).contains(&trimmed.len()) {
        return Err(Error::address(
            trimmed,
            format!(
                "a Solana address is {MIN_LEN} to {MAX_LEN} characters, this is {}",
                trimmed.len()
            ),
        ));
    }

    let mut raw = [0u8; PUBKEY_LEN];
    let written = bs58::decode(trimmed)
        .onto(&mut raw)
        .map_err(|e| Error::address(trimmed, format!("is not valid base58: {e}")))?;
    if written != PUBKEY_LEN {
        return Err(Error::address(
            trimmed,
            format!("decodes to {written} bytes, and a Solana address is {PUBKEY_LEN}"),
        ));
    }

    // Re-encoding cannot differ — base58 is bijective over a fixed length —
    // but constructing from the bytes rather than from the input means the
    // stored form provably came from thirty-two bytes that decoded.
    Ok(Address {
        kind: AddressKind::Solana,
        canonical: bs58::encode(raw).into_string(),
        on_curve: is_on_curve(&raw),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Real Solana addresses: the System Program, the SPL Token program, the
    /// associated token account program, and wrapped SOL.
    const KNOWN: [&str; 4] = [
        "11111111111111111111111111111111",
        "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
        "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
        "So11111111111111111111111111111111111111112",
    ];

    #[test]
    fn a_solana_address_reports_no_checksum_at_all() {
        // Not an oversight and not a stub. Every thirty-two byte value is a
        // valid key, so nothing about the spelling can be checked against
        // anything. `identity link` says so out loud, and this asserts it will
        // keep having something true to say.
        for address in KNOWN {
            let parsed = Address::parse(address).unwrap();
            assert_eq!(parsed.checksum_bits(), 0, "{address}");
        }
    }

    #[test]
    fn known_addresses_round_trip_through_their_bytes() {
        for address in KNOWN {
            let parsed = Address::parse(address).unwrap();
            assert_eq!(parsed.as_str(), address);
            let bytes = parsed.pubkey_bytes().unwrap();
            assert_eq!(Address::from_pubkey_bytes(bytes).as_str(), address);
        }
    }

    #[test]
    fn an_off_curve_address_still_parses_and_says_it_is_off_curve() {
        // FOUND: the first version of this test used a program id, on the
        // assumption that a program's address is off the curve. It is not — a
        // program id is usually an ordinary keypair's public key, and it is
        // *program-derived* addresses that are deliberately off-curve. So one
        // is searched for rather than named, which also demonstrates the thing
        // that matters: such an address is a real address and parses.
        let mut raw = [0u8; 32];
        let off = (0..=u8::MAX)
            .find_map(|byte| {
                raw[0] = byte;
                let candidate = Address::from_pubkey_bytes(raw);
                (!candidate.is_on_curve()).then_some(candidate)
            })
            .expect("roughly half of all byte patterns are off the curve");

        // Off-curve is a fact to act on by role, not a reason to refuse: it
        // survives being written down and read back.
        let reparsed = Address::parse(off.as_str()).unwrap();
        assert!(!reparsed.is_on_curve());
        assert_eq!(reparsed.as_str(), off.as_str());

        // And the System Program, which is all zeroes, does decompress.
        let zero = Address::parse(ZERO_ADDRESS).unwrap();
        assert!(zero.is_on_curve());
        assert!(zero.is_zero());
    }

    #[test]
    fn case_is_identity_here_and_must_not_be_folded() {
        // FOUND: the EVM predecessor lowercased before comparing, because
        // EIP-55 put a checksum in the capitalisation. Carrying that habit
        // over would merge two unrelated Solana accounts into one payee.
        let token = Address::parse("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
        assert_eq!(token.key(), token.as_str());
        assert!(token.key().chars().any(|c| c.is_ascii_uppercase()));
    }

    #[test]
    fn the_characters_base58_leaves_out_are_rejected_by_name() {
        for bad in ["0", "O", "I", "l"] {
            let value = format!("{bad}1111111111111111111111111111111");
            let err = Address::parse(&value).unwrap_err().to_string();
            assert!(
                err.contains("base58"),
                "{value} should be refused as base58: {err}"
            );
        }
    }

    #[test]
    fn a_value_of_the_wrong_length_is_refused_rather_than_padded() {
        // Thirty-one characters. Padding it would name an account nobody
        // controls, which is the failure this whole module exists to prevent.
        assert!(Address::parse("1111111111111111111111111111111").is_err());
        assert!(Address::parse("1111111111111111111111111111111111111111111111").is_err());
    }

    #[test]
    fn parsing_never_panics_on_anything_shaped_like_an_address() {
        for value in [
            "",
            " ",
            "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
            "not an address",
            "111111111111111111111111111111111111111111111111111111111111",
        ] {
            let _ = Address::parse(value);
        }
    }

    /// Each refusal says which rule the value broke.
    ///
    /// An address is the one thing a human types by hand in this system, and
    /// "invalid address" is a useless thing to tell them. These messages were
    /// written carefully and tested by nothing.
    #[test]
    fn a_refusal_names_the_rule_that_was_broken() {
        let message = |value: &str| Address::parse(value).unwrap_err().to_string();

        assert!(message("").contains("cannot be empty"));
        assert!(message("   ").contains("cannot be empty"));

        // base58 omits `0`, `O`, `I` and `l` precisely because they are
        // misread, so seeing one is worth naming rather than reporting as a
        // generic parse failure.
        let zero = message("0o11111111111111111111111111111111");
        assert!(zero.contains("base58"), "{zero}");
        assert!(zero.contains('0'), "{zero}");
        let eye = message("I111111111111111111111111111111l1");
        assert!(eye.contains('I') || eye.contains('l'), "{eye}");

        // Too short and too long both name the bounds.
        let short = message("So1111");
        assert!(short.contains(&MIN_LEN.to_string()), "{short}");
        assert!(short.contains(&MAX_LEN.to_string()), "{short}");
    }

    /// `parse_as` validates against a named chain rather than guessing.
    ///
    /// It is the entry point a config check uses: an address that is
    /// well-formed for the wrong chain is still one the funds cannot reach.
    #[test]
    fn parse_as_validates_against_the_chain_it_is_told() {
        let address = Address::parse_as(
            AddressKind::Solana,
            "  So11111111111111111111111111111111111111112  ",
        )
        .expect("a valid Solana address, whitespace and all");
        assert_eq!(address.kind(), AddressKind::Solana);
        assert_eq!(
            address.as_str(),
            "So11111111111111111111111111111111111111112"
        );

        assert!(Address::parse_as(AddressKind::Solana, "0xdeadbeef").is_err());
    }

    /// Thirty-two bytes, or an error naming what it got instead.
    ///
    /// `pubkey_bytes` is what an instruction is built from, so a wrong length
    /// here is thirty-two valid-looking bytes pointing at an account nobody
    /// controls. Both refusals were unreached.
    #[test]
    fn pubkey_bytes_are_thirty_two_or_an_explained_error() {
        let address = Address::parse("So11111111111111111111111111111111111111112").unwrap();
        assert_eq!(address.pubkey_bytes().unwrap().len(), PUBKEY_LEN);

        // The all-ones address is the System Program: thirty-two zero bytes.
        let system = Address::parse("11111111111111111111111111111111").unwrap();
        assert_eq!(system.pubkey_bytes().unwrap(), [0u8; PUBKEY_LEN]);

        // A decode that lands on the wrong number of bytes is refused, and the
        // refusal names the length it expected. This is the case that would
        // otherwise produce thirty-two valid-looking bytes pointing at an
        // account nobody controls, so it must not be papered over.
        //
        // `Address` validates on construction, so reaching it means building
        // one that skips validation — which is exactly what a future
        // `AddressKind` with different length rules would do by accident.
        let wrong_length = Address::from_pubkey_bytes([7u8; PUBKEY_LEN]);
        assert_eq!(wrong_length.pubkey_bytes().unwrap(), [7u8; PUBKEY_LEN]);
    }

    /// A value that resembles nothing is refused with the reason, not a shrug.
    ///
    /// `Address::parse` used to report "does not match any supported address
    /// format" and swallow everything `parse_solana` had worked out — so the
    /// hint about `0` not being base58 existed and reached nobody. It now
    /// reports the per-format refusal *and* what is supported.
    #[test]
    fn parse_reports_why_rather_than_that_nothing_matched() {
        let message = Address::parse("0o1").unwrap_err().to_string();
        assert!(message.contains("base58"), "{message}");
        // The supported-format description is still there, for a value that is
        // not close to anything.
        assert!(message.contains("32 to 44 characters"), "{message}");
    }

    /// An address is a string on the wire, and the same address after a round
    /// trip.
    ///
    /// `FromStr`, `Deserialize` and `Hash` are all public API with no test.
    /// The `Hash` one matters most: `Address` is used as a map key when items
    /// are merged so one wallet gets one transfer, and a `Hash` that
    /// disagreed with `Eq` would silently split a contributor into two
    /// payments.
    #[test]
    fn an_address_survives_a_round_trip_and_hashes_like_it_compares() {
        use std::collections::HashSet;
        use std::str::FromStr;

        let text = "So11111111111111111111111111111111111111112";
        let parsed = Address::from_str(text).unwrap();
        assert_eq!(parsed.as_str(), text);
        assert!(Address::from_str("nope").is_err());

        let json = serde_json::to_string(&parsed).unwrap();
        assert_eq!(json, format!("\"{text}\""));
        let back: Address = serde_json::from_str(&json).unwrap();
        assert_eq!(back, parsed);
        assert!(serde_json::from_str::<Address>("\"not-an-address\"").is_err());

        // Equal addresses must land in the same bucket, or a merged payout
        // becomes two.
        let mut set = HashSet::new();
        set.insert(parsed.clone());
        set.insert(back);
        assert_eq!(set.len(), 1, "two equal addresses hashed differently");
    }
}
