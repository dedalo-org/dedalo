//! EVM settlement: what Dedalo does *not* do.
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
//! The backend stays so that `settlement.backend = "evm"` in an existing
//! config fails with an explanation rather than an unknown-backend error, and
//! so the chain settings it validates are checked in one place.
//!
//! [`docs/settlement-architecture.md`]: https://github.com/dedalo-org/dedalo/blob/main/docs/settlement-architecture.md

use async_trait::async_trait;

use super::{Settlement, SettlementReceipt};
use crate::config::SettlementConfig;
use crate::error::{Error, Result};
use crate::money::{Amount, Asset};
use crate::payout::PayoutPlan;

/// Chain settings for a round that will be funded from a multisig.
#[derive(Debug, Clone)]
pub struct EvmSettlement {
    chain_id: u64,
    contract: String,
}

impl EvmSettlement {
    /// Build a backend, requiring the settings a proposal needs.
    ///
    /// `rpc_url` is not required: nothing here opens a connection.
    pub fn from_config(config: &SettlementConfig) -> Result<Self> {
        let chain_id = config
            .chain_id
            .ok_or_else(|| Error::config("settlement.chain_id is required for the evm backend"))?;
        let contract = config
            .contract
            .clone()
            .ok_or_else(|| Error::config("settlement.contract is required for the evm backend"))?;
        Ok(Self { chain_id, contract })
    }

    /// The configured EIP-155 chain id.
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// The claim contract a round is deposited into.
    pub fn contract(&self) -> &str {
        &self.contract
    }
}

#[async_trait]
impl Settlement for EvmSettlement {
    fn name(&self) -> &str {
        "evm"
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
            feature: "evm broadcast",
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
            backend: "evm".into(),
            rpc_url: Some("https://mainnet.base.org".into()),
            chain_id: Some(8453),
            contract: Some("0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359".into()),
        }
    }

    #[test]
    fn requires_the_chain_settings_a_proposal_needs() {
        let mut incomplete = config();
        incomplete.chain_id = None;
        assert!(EvmSettlement::from_config(&incomplete).is_err());

        let mut incomplete = config();
        incomplete.contract = None;
        assert!(EvmSettlement::from_config(&incomplete).is_err());

        assert!(EvmSettlement::from_config(&config()).is_ok());
    }

    /// An rpc_url is not needed, because nothing here connects to anything.
    #[test]
    fn an_rpc_url_is_optional() {
        let mut without = config();
        without.rpc_url = None;
        assert!(EvmSettlement::from_config(&without).is_ok());
    }
}
