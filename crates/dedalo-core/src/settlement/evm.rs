//! EVM settlement: batched transfers through a distributor contract.
//!
//! # Status
//!
//! This backend is **wired but not yet able to broadcast**. It validates the
//! configuration, re-verifies the plan and produces the exact call payload a
//! signer would submit, then stops before signing. Shipping a half-tested
//! signing path would risk real funds, so [`Settlement::settle`] deliberately
//! returns [`Error::NotImplemented`] until the on-chain contract is audited
//! and the signer is integrated.
//!
//! Use `--dry-run` (the default) meanwhile: it produces the same plan and the
//! same numbers, minus the broadcast.
//!
//! # Intended on-chain shape
//!
//! ```solidity
//! function distribute(
//!     bytes16 planId,      // content hash of the payout plan
//!     address token,       // address(0) for the native coin
//!     address[] payees,
//!     uint256[] amounts
//! ) external;
//! ```
//!
//! Passing `planId` on chain is what ties a transaction back to a reviewable
//! artifact in git, and lets the contract reject a plan that was already paid.

use async_trait::async_trait;

use super::{Settlement, SettlementReceipt};
use crate::config::SettlementConfig;
use crate::error::{Error, Result};
use crate::money::{Amount, Asset};
use crate::payout::PayoutPlan;

/// Settlement through a distributor contract on an EVM chain.
#[derive(Debug, Clone)]
pub struct EvmSettlement {
    rpc_url: String,
    chain_id: u64,
    contract: String,
    signer_env: String,
}

impl EvmSettlement {
    /// Build a backend, requiring every chain setting to be present.
    pub fn from_config(config: &SettlementConfig) -> Result<Self> {
        let rpc_url = config
            .rpc_url
            .clone()
            .ok_or_else(|| Error::config("settlement.rpc_url is required for the evm backend"))?;
        let chain_id = config
            .chain_id
            .ok_or_else(|| Error::config("settlement.chain_id is required for the evm backend"))?;
        let contract = config
            .contract
            .clone()
            .ok_or_else(|| Error::config("settlement.contract is required for the evm backend"))?;
        Ok(Self {
            rpc_url,
            chain_id,
            contract,
            signer_env: config.signer_env.clone(),
        })
    }

    /// The configured JSON-RPC endpoint.
    pub fn rpc_url(&self) -> &str {
        &self.rpc_url
    }

    /// The configured EIP-155 chain id.
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// The distributor contract address.
    pub fn contract(&self) -> &str {
        &self.contract
    }

    /// Whether a signing key is present in the environment. Checked before a
    /// round starts so a long CI job does not fail at the last step.
    pub fn has_signer(&self) -> bool {
        std::env::var(&self.signer_env)
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
    }

    /// The `(payees, amounts)` arrays that would be sent to `distribute`.
    ///
    /// Exposed so the CLI can print exactly what will be broadcast, and so
    /// the calldata can be diffed against the plan by a reviewer.
    pub fn call_arguments(&self, plan: &PayoutPlan) -> Result<CallArguments> {
        plan.verify()?;
        let mut payees = Vec::new();
        let mut amounts = Vec::new();
        for item in plan.payable_items() {
            payees.push(item.wallet.clone());
            amounts.push(item.amount);
        }
        Ok(CallArguments {
            plan_id: plan.id.clone(),
            token: plan.asset.contract.clone(),
            contract: self.contract.clone(),
            chain_id: self.chain_id,
            payees,
            amounts,
        })
    }
}

/// The distributor call a plan translates into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallArguments {
    /// Content hash of the plan, passed on chain to tie the transaction to it.
    pub plan_id: String,
    /// `None` means the chain's native coin.
    pub token: Option<String>,
    /// Distributor contract being called.
    pub contract: String,
    /// Chain the call targets.
    pub chain_id: u64,
    /// Recipient addresses, in plan order.
    pub payees: Vec<String>,
    /// Amounts, positionally matched to `payees`.
    pub amounts: Vec<Amount>,
}

impl CallArguments {
    /// Sum of every amount in the call.
    pub fn total(&self) -> Result<Amount> {
        self.amounts
            .iter()
            .try_fold(Amount::ZERO, |acc, amount| acc.checked_add(*amount))
    }
}

#[async_trait]
impl Settlement for EvmSettlement {
    fn name(&self) -> &str {
        "evm"
    }

    async fn balance(&self, _asset: &Asset) -> Result<Option<Amount>> {
        // Reporting a balance requires the same RPC client as broadcasting.
        Ok(None)
    }

    async fn settle(&self, plan: &PayoutPlan) -> Result<SettlementReceipt> {
        // Validate everything that can be validated offline, so the error the
        // user sees is about the missing broadcast and nothing else.
        let args = self.call_arguments(plan)?;
        if args.payees.is_empty() {
            return Err(Error::Settlement {
                backend: self.name().to_string(),
                reason: "plan contains no non-zero transfers".into(),
            });
        }
        if !self.has_signer() {
            return Err(Error::Settlement {
                backend: self.name().to_string(),
                reason: format!("no signing key in ${}", self.signer_env),
            });
        }
        Err(Error::NotImplemented {
            feature: "evm broadcast",
            hint: "the distributor contract is not deployed yet; run with --dry-run to review the plan",
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
            contract: Some("0xdistributor".into()),
            signer_env: "DEDALO_TEST_SIGNER".into(),
        }
    }

    #[test]
    fn requires_full_chain_configuration() {
        let mut incomplete = config();
        incomplete.rpc_url = None;
        assert!(EvmSettlement::from_config(&incomplete).is_err());
        assert!(EvmSettlement::from_config(&config()).is_ok());
    }
}
