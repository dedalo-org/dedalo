//! Mapping git identities to payable wallets.
//!
//! A commit knows an email; a blockchain knows an address. Nothing in git
//! proves the link, so it is declared explicitly in `dedalo.toml` and kept
//! auditable in the repo alongside the code.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::chain::wallet::Address;
use crate::git::Author;

/// A contributor and the wallet their share is sent to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// Human handle used in reports, e.g. a GitHub username.
    pub handle: String,
    /// Destination address for payouts. `None` for an excluded identity,
    /// which earns attribution but is never paid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wallet: Option<Address>,
    /// Every git email that belongs to this contributor.
    #[serde(default)]
    pub emails: Vec<String>,
    /// Excluded contributors still earn attribution but are never paid, e.g.
    /// bots or employees already compensated elsewhere.
    #[serde(default)]
    pub excluded: bool,
}

impl Identity {
    /// A payable identity with no emails attached yet.
    pub fn new(handle: impl Into<String>, wallet: Address) -> Self {
        Self {
            handle: handle.into(),
            wallet: Some(wallet),
            emails: Vec::new(),
            excluded: false,
        }
    }

    /// Build an identity from an address that has not been validated yet.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Address`] if `wallet` is not a usable
    /// address — including one that decodes to the wrong number of bytes,
    /// which is what a typo looks like.
    pub fn parse(handle: impl Into<String>, wallet: &str) -> crate::error::Result<Self> {
        Ok(Self::new(handle, Address::parse(wallet)?))
    }

    /// Attach a git email to this identity.
    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.emails.push(email.into());
        self
    }
}

/// Resolves commit authors to identities, by email.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IdentityMap {
    identities: Vec<Identity>,
}

impl IdentityMap {
    /// Wrap a list of identities.
    pub fn new(identities: Vec<Identity>) -> Self {
        Self { identities }
    }

    /// Whether any identity is registered at all.
    pub fn is_empty(&self) -> bool {
        self.identities.is_empty()
    }

    /// Iterate over every identity, in declaration order.
    pub fn iter(&self) -> impl Iterator<Item = &Identity> {
        self.identities.iter()
    }

    /// Index from lowercased email to identity, built on demand.
    pub fn by_email(&self) -> BTreeMap<String, &Identity> {
        let mut map = BTreeMap::new();
        for identity in &self.identities {
            for email in &identity.emails {
                map.insert(email.trim().to_ascii_lowercase(), identity);
            }
        }
        map
    }

    /// Find the identity owning a commit author's email.
    pub fn resolve(&self, author: &Author) -> Option<&Identity> {
        let key = author.key();
        self.identities.iter().find(|identity| {
            identity
                .emails
                .iter()
                .any(|e| e.trim().eq_ignore_ascii_case(&key))
        })
    }

    /// Look an identity up by handle.
    pub fn find_handle(&self, handle: &str) -> Option<&Identity> {
        self.identities.iter().find(|i| i.handle == handle)
    }

    /// Add an identity, or attach the email to the matching handle if it
    /// already exists. Returns `true` when something actually changed.
    pub fn link(&mut self, handle: &str, wallet: Address, email: &str) -> bool {
        let email = email.trim().to_ascii_lowercase();
        if let Some(existing) = self.identities.iter_mut().find(|i| i.handle == handle) {
            let mut changed = false;
            if existing.wallet.as_ref() != Some(&wallet) {
                existing.wallet = Some(wallet);
                changed = true;
            }
            if !existing
                .emails
                .iter()
                .any(|e| e.eq_ignore_ascii_case(&email))
            {
                existing.emails.push(email);
                changed = true;
            }
            return changed;
        }
        self.identities
            .push(Identity::new(handle, wallet).with_email(email));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_authors_case_insensitively() {
        let map = IdentityMap::new(vec![
            Identity::parse("ada", "So11111111111111111111111111111111111111112")
                .unwrap()
                .with_email("Ada@Example.COM"),
        ]);
        let author = Author::new("Ada L", "ada@example.com");
        assert_eq!(map.resolve(&author).unwrap().handle, "ada");
    }

    #[test]
    fn link_merges_into_existing_handle() {
        let mut map = IdentityMap::default();
        let wallet = || Address::parse("So11111111111111111111111111111111111111112").unwrap();
        assert!(map.link("ada", wallet(), "ada@example.com"));
        assert!(map.link("ada", wallet(), "ada@work.com"));
        assert!(!map.link("ada", wallet(), "ada@work.com"));
        assert_eq!(map.iter().count(), 1);
        assert_eq!(map.find_handle("ada").unwrap().emails.len(), 2);
    }

    /// Relinking the same handle to a *different* wallet moves the money.
    ///
    /// This is the branch nothing reached, and it is the one that matters:
    /// `identity link` on an existing handle silently repoints where that
    /// person is paid. It has to actually change the wallet, and it has to
    /// report that it changed something, because the caller decides whether to
    /// write the config back.
    #[test]
    fn relinking_a_handle_repoints_the_wallet() {
        let first = Address::parse("So11111111111111111111111111111111111111112").unwrap();
        let second = Address::parse("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();

        let mut map = IdentityMap::default();
        assert!(map.link("ada", first.clone(), "ada@example.com"));
        assert_eq!(
            map.find_handle("ada").unwrap().wallet.as_ref(),
            Some(&first)
        );

        assert!(
            map.link("ada", second.clone(), "ada@example.com"),
            "changing the wallet is a change"
        );
        assert_eq!(
            map.find_handle("ada").unwrap().wallet.as_ref(),
            Some(&second)
        );
        // One identity still, not two.
        assert_eq!(map.iter().count(), 1);
    }

    /// `parse` refuses a wallet that is not one, rather than storing a string.
    #[test]
    fn an_identity_cannot_be_built_from_a_wallet_that_is_not_one() {
        assert!(Identity::parse("ada", "not-an-address").is_err());
        assert!(Identity::parse("ada", "").is_err());
        assert!(Identity::parse("ada", "So11111111111111111111111111111111111111112").is_ok());
    }

    /// The email index is keyed case-insensitively and trimmed.
    ///
    /// `by_email` is what a caller uses to answer "who is this commit author",
    /// and git emails arrive with whatever whitespace and capitalisation the
    /// committer's config had. A lookup that missed on `Ada@Example.com` would
    /// leave a linked contributor unpaid.
    #[test]
    fn the_email_index_ignores_case_and_whitespace() {
        let wallet = Address::parse("So11111111111111111111111111111111111111112").unwrap();
        let map = IdentityMap::new(vec![
            Identity::new("ada", wallet.clone())
                .with_email("  Ada@Example.COM ")
                .with_email("ada@work.io"),
        ]);

        assert!(!map.is_empty());
        let index = map.by_email();
        assert_eq!(index.len(), 2);
        assert_eq!(index.get("ada@example.com").unwrap().handle, "ada");
        assert_eq!(index.get("ada@work.io").unwrap().handle, "ada");

        assert!(IdentityMap::default().is_empty());
        assert!(IdentityMap::default().by_email().is_empty());
    }
}
