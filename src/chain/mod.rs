//! Everything that faces a chain.
//!
//! The boundary this directory draws is the one
//! [`docs/settlement-architecture.md`] draws: below it, amounts are decided;
//! here, they are turned into something a chain can check. Nothing in here
//! chooses who is paid what.
//!
//! - [`wallet`] validates a destination before it is written down.
//! - [`merkle`] builds the tree a round is deposited against.
//! - [`vault`] is what the contract enforces, as pure Rust — the rules for
//!   depositing a round, claiming a share and recovering what nobody claimed.
//! - [`settlement`] turns a plan into transactions for a person to sign.
//!
//! The deployable contract lives at `src/chain/contract`. It is a separate
//! crate because it compiles to WebAssembly for a different target, and it is
//! deliberately thin: the rules it enforces are [`vault`], tested here with
//! the same machinery as the rest of the money path.
//!
//! [`docs/settlement-architecture.md`]: https://github.com/dedalo-org/dedalo/blob/main/docs/settlement-architecture.md

pub mod merkle;
pub mod settlement;
pub mod vault;
pub mod wallet;
