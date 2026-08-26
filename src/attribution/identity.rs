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
}
