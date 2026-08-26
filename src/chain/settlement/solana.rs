//! Solana settlement: what Dedalo does *not* do.
//!
//! # Status
//!
//! This backend broadcasts nothing, and after
//! [`docs/settlement-architecture.md`] it is not going to. Two decisions
//! recorded there make a broadcasting backend the wrong thing to build:
//!
//! - **Pull, not push.** A round is deposited once against a Merkle root and
//!   contributors claim. There is no batched transfer for this backend to
//!   send.
//! - **The key is not in CI.** A signing key in a workflow environment can
//!   drain the source wallet, and everything with write access to a workflow
//!   can reach it. Dedalo therefore holds no key and produces no signature.
//!
//! What replaces it is [`crate::chain::settlement::proposal`]: `dedalo propose`
//! emits the exact transactions a multisig must run, calldata included, and
//! people sign them.
//!
//! The backend stays so that `settlement.backend = "solana"` in a config
//! fails with an explanation rather than an unknown-backend error, and so the
//! cluster settings it validates are checked in one place.
//!
//! There is no program to broadcast to yet: the claim program is unwritten,
//! and the vault rules that will go into it live in [`crate::chain::vault`].
//!
//! [`docs/settlement-architecture.md`]: https://github.com/dedalo-org/dedalo/blob/main/docs/settlement-architecture.md

use async_trait::async_trait;

use super::{Settlement, SettlementReceipt};
use crate::chain::wallet::Address;
use crate::config::SettlementConfig;
use crate::error::{Error, Result};
use crate::money::{Amount, Asset};
use crate::payout::PayoutPlan;

/// Chain settings for a round that will be funded from a multisig.
#[derive(Debug, Clone)]
pub struct SolanaSettlement {
    cluster: String,
    program_id: Address,
}

impl SolanaSettlement {
    /// Build a backend, requiring the settings a proposal needs.
    ///
    /// `rpc_url` is not required: nothing here opens a connection.
    pub fn from_config(config: &SettlementConfig) -> Result<Self> {
        let cluster = config.cluster.clone().ok_or_else(|| {
            Error::config("settlement.cluster is required for the solana backend")
        })?;
        let raw = config.program_id.clone().ok_or_else(|| {
            Error::config("settlement.program_id is required for the solana backend")
        })?;
        // A program id is an address, and is validated as one: a program id
        // that is not thirty-two bytes names nothing, and finding that out at
        // config time beats finding it out from a signer.
        let program_id = Address::parse(&raw)?;
        Ok(Self {
            cluster,
            program_id,
        })
    }

    /// The configured cluster: `mainnet-beta`, `devnet` or `testnet`.
    pub fn cluster(&self) -> &str {
        &self.cluster
    }

    /// The claim program a round is deposited into.
    pub fn program_id(&self) -> &Address {
        &self.program_id
    }
}

#[async_trait]
impl Settlement for SolanaSettlement {
    fn name(&self) -> &str {
        "solana"
    }

    async fn balance(&self, _asset: &Asset) -> Result<Option<Amount>> {
        // Would need an RPC client, which this crate deliberately does not
        // carry: the multisig reports its own balance, and it is the thing
        // that actually pays.
        Ok(None)
    }

    async fn settle(&self, plan: &PayoutPlan) -> Result<SettlementReceipt> {
        // Validated first, so the error names the missing capability rather
        // than something incidental about the plan.
        plan.verify()?;
        Err(Error::NotImplemented {
            feature: "solana broadcast",
            hint: "Dedalo holds no signing key by design; run `dedalo propose` and \
                   execute the transactions it prints from the project's multisig",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> SettlementConfig {
        SettlementConfig {
            backend: "solana".into(),
            rpc_url: Some("https://api.devnet.solana.com".into()),
            cluster: Some("devnet".into()),
            program_id: Some("MerkS3LaQBSvM5JZsvBaLZBBSMvMB5aTuLRHrvKAyDo".into()),
        }
    }

    #[test]
    fn requires_the_cluster_settings_a_proposal_needs() {
        let mut incomplete = config();
        incomplete.cluster = None;
        assert!(SolanaSettlement::from_config(&incomplete).is_err());

        let mut incomplete = config();
        incomplete.program_id = None;
        assert!(SolanaSettlement::from_config(&incomplete).is_err());

        assert!(SolanaSettlement::from_config(&config()).is_ok());
    }

    /// A program id is an address, so a malformed one is refused here rather
    /// than reaching a proposal a signer would be asked to trust.
    #[test]
    fn a_program_id_that_is_not_an_address_is_refused() {
        let mut wrong = config();
        wrong.program_id = Some("not-a-program".into());
        assert!(SolanaSettlement::from_config(&wrong).is_err());
    }

    /// An rpc_url is not needed, because nothing here connects to anything.
    #[test]
    fn an_rpc_url_is_optional() {
        let mut without = config();
        without.rpc_url = None;
        assert!(SolanaSettlement::from_config(&without).is_ok());
    }
}
