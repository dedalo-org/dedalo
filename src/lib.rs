//! # Dedalo
//!
//! **This version is a placeholder.** `0.0.0` reserves the name on crates.io.
//! It exposes nothing, depends on nothing, and does nothing.
//!
//! What it will be — merges already in a git repository, turned into a
//! deterministic, auditable payout plan and settled on chain — is being built
//! on the [`v0.1`] branch, and reaches crates.io as `0.0.1` once it is stable
//! enough to be worth installing.
//!
//! Publishing an empty `0.0.0` rather than the work in progress is the point.
//! A version on crates.io can be yanked but never withdrawn, and someone who
//! runs `cargo add dedalo` today should get something that is honestly empty
//! rather than something that half-computes what people are owed.
//!
//! [`v0.1`]: https://github.com/dedalo-org/dedalo/tree/v0.1

// Kept from the real crate rather than dropped: the moment a public item
// lands here again, it has to be documented, and CI builds rustdoc with
// `-D warnings`. A ratchet is only a ratchet if it stays on.
#![warn(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]
