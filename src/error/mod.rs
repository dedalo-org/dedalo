//! Error and result types shared by every stage.
//!
//! One enum for the whole crate rather than one per module: a caller that has
//! to match on four unrelated error types to find out whether a round can
//! proceed will match on none of them.

use std::path::PathBuf;

/// Result alias used across the whole core crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything that can go wrong between a git history and a settled round.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A file could not be read or written.
    #[error("io error at {path}: {source}")]
    Io {
        /// The path that could not be accessed.
        path: PathBuf,
        /// The underlying filesystem error.
        #[source]
        source: std::io::Error,
    },

    /// The `git` binary ran but exited non-zero.
    #[error("git command `git {args}` failed: {stderr}")]
    Git {
        /// Arguments passed to `git`, for reproducing the failure by hand.
        args: String,
        /// What git wrote to stderr.
        stderr: String,
    },

    /// No `git` executable is available. Dedalo cannot work without one.
    #[error("`git` executable not found in PATH: {0}")]
    GitMissing(#[source] std::io::Error),

    /// Git succeeded but produced output this version cannot parse.
    #[error("unexpected git output while parsing {context}: {detail}")]
    GitParse {
        /// What was being read, e.g. `merge history`.
        context: String,
        /// Why the output could not be interpreted.
        detail: String,
    },

    /// The configuration is syntactically valid but semantically wrong.
    #[error("config error: {0}")]
    Config(String),

    /// `dedalo.toml` is not valid TOML, or has the wrong shape.
    #[error("failed to parse {path}: {source}")]
    ConfigParse {
        /// The config file that failed to parse.
        path: PathBuf,
        /// The underlying TOML error, with its span.
        #[source]
        source: toml::de::Error,
    },

    /// No `dedalo.toml` exists anywhere up the directory tree.
    #[error("no dedalo.toml found in {0} or any parent directory (run `dedalo init`)")]
    ConfigNotFound(PathBuf),

    /// A ledger entry, plan or receipt could not be (de)serialized.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// A monetary amount could not be parsed at the asset's precision.
    #[error("amount `{value}` is not a valid decimal with {decimals} decimals")]
    Amount {
        /// The text that was rejected.
        value: String,
        /// Decimal places the asset actually uses.
        decimals: u8,
    },

    /// A payout destination is not a usable address.
    #[error("`{value}` is not a valid address: {reason}")]
    Address {
        /// The text that was rejected.
        value: String,
        /// Why it was rejected.
        reason: String,
    },

    /// An arithmetic operation on money or weights would have wrapped.
    #[error("arithmetic overflow while computing {0}")]
    Overflow(&'static str),

    /// A commit author has no wallet mapped in the configuration.
    #[error("unknown contributor identity for `{0}` (run `dedalo identity link`)")]
    UnknownIdentity(String),

    /// A settlement backend refused to execute the plan.
    #[error("settlement backend `{backend}` rejected the plan: {reason}")]
    Settlement {
        /// Backend that refused, e.g. `evm`.
        backend: String,
        /// Why it refused.
        reason: String,
    },

    /// The ledger chain does not hash to what it claims.
    ///
    /// This is not a parse failure: it means the record was changed after it
    /// was written, or that an entry it points at is missing.
    #[error("ledger is corrupt at {id}: {reason}")]
    LedgerCorrupt {
        /// Id of the entry the walk stopped at.
        id: String,
        /// What did not add up.
        reason: String,
    },

    /// The requested capability exists in the API but is not live yet.
    #[error("{feature} is not implemented yet in this release: {hint}")]
    NotImplemented {
        /// The capability that is missing.
        feature: &'static str,
        /// What to do instead, in the meantime.
        hint: &'static str,
    },
}

impl Error {
    /// Wrap a filesystem error with the path it happened on.
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }

    /// Build an address error.
    pub fn address(value: impl Into<String>, reason: impl Into<String>) -> Self {
        Error::Address {
            value: value.into(),
            reason: reason.into(),
        }
    }

    /// Build a configuration error from a message.
    pub fn config(msg: impl Into<String>) -> Self {
        Error::Config(msg.into())
    }
}
