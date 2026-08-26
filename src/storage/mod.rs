//! `.dedalo/`, on disk.
//!
//! [`objects`] is the content-addressed store — the git-shaped part: objects,
//! refs, `HEAD`. [`ledger`] is what is kept in it: a hash chain of everything
//! Dedalo did, which is the reason an entry cannot be edited after the fact
//! without every id since changing.

pub mod ledger;
pub mod objects;
