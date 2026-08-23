//! What a round asks a human to sign.
//!
//! [`docs/settlement-architecture.md`] decided that the pipeline proposes and
//! people execute: a signing key in CI can drain the treasury, and everything
//! with write access to a workflow can reach it. So Dedalo produces no
//! signature and holds no key. It produces this — the exact transactions a
//! multisig must run, in order, with the calldata already encoded so a signer
//! can compare it against the plan rather than trust it.
//!
//! ```text
//! dedalo plan     ─▶  a reviewed PayoutPlan, content-addressed
//! dedalo propose  ─▶  1. approve(claimContract, total)
//!                     2. deposit(planId, merkleRoot, token, total)
//!                     ↓
//!                 a Safe, signed by people who are not one person
//!                     ↓
//!                 contributors claim, each paying their own gas
//! ```
//!
//! Everything here is offline. Nothing in this module opens a socket.
//!
//! [`docs/settlement-architecture.md`]: https://github.com/dedalo-org/dedalo/blob/main/docs/settlement-architecture.md

use serde::{Deserialize, Serialize};

use crate::chain::merkle::ClaimTree;
use crate::chain::settlement::abi;
use crate::chain::wallet::Address;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::money::Amount;
use crate::payout::PayoutPlan;

/// One transaction for a signer to execute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionProposal {
    /// Position in the sequence. They are not independent: a deposit before
    /// its approval reverts.
    pub step: u32,
    /// What this transaction does, for the person deciding whether to sign.
    pub description: String,
    /// EIP-155 chain this must be executed on. A signer with several chains
    /// configured needs to be told which, not left to guess.
    pub chain_id: u64,
    /// Contract to call.
    pub to: String,
    /// Native coin sent along, in base units. Zero for every ERC-20 step.
    pub value: Amount,
    /// ABI-encoded calldata, `0x`-prefixed.
    pub data: String,
}

/// Everything a signer needs to fund one round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundProposal {
    /// The plan being funded. Passed on chain, so a round can be paid once.
    pub plan_id: String,
    /// Root of the tree contributors prove against, `0x`-prefixed.
    pub merkle_root: String,
    /// Contract that holds the deposit and pays claims.
    pub claim_contract: String,
    /// Token being distributed, or `None` for the chain's native coin.
    pub token: Option<String>,
    /// Sum of every claim. What the deposit must cover exactly.
    pub total: Amount,
    /// How many contributors can claim.
    pub claims: usize,
    /// The transactions, in the order they must run.
    pub transactions: Vec<TransactionProposal>,
}

impl RoundProposal {
    /// Build the proposal for a plan, from the project's settlement config.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] if the settlement section does not name a
    /// chain id and a claim contract, and [`Error::Config`] if the plan pays
    /// nobody — there is nothing to deposit against.
    ///
    /// Returns [`Error::NotImplemented`] for a native-coin round: the deposit
    /// path for a chain's own coin is a different call with a `value`, and
    /// writing it untested would be guessing at how money moves.
    pub fn build(plan: &PayoutPlan, config: &Config) -> Result<Self> {
        plan.verify()?;

        let settlement = &config.settlement;
        let chain_id = settlement
            .chain_id
            .ok_or_else(|| Error::config("settlement.chain_id is required to propose a round"))?;
        let claim_contract = settlement
            .contract
            .as_deref()
            .ok_or_else(|| Error::config("settlement.contract is required to propose a round"))?;
        let claim_contract = Address::parse(claim_contract)?;

        let token = match plan.asset.contract.as_deref() {
            Some(contract) => Address::parse(contract)?,
            None => {
                return Err(Error::NotImplemented {
                    feature: "native-coin rounds",
                    hint: "set asset.contract to an ERC-20; depositing the chain's own coin \
                           is a different call and is not written yet",
                });
            }
        };

        let tree = ClaimTree::from_plan(plan)?;
        let total = tree.total()?;

        let transactions = vec![
            TransactionProposal {
                step: 1,
                description: format!(
                    "approve {} to move {} {}",
                    claim_contract.as_str(),
                    plan.asset.format_amount(total),
                    plan.asset.symbol
                ),
                chain_id,
                to: token.as_str().to_string(),
                value: Amount::ZERO,
                data: hex_data(&abi::approve_calldata(&claim_contract, total)?),
            },
            TransactionProposal {
                step: 2,
                description: format!(
                    "deposit round {} against root {}",
                    plan.short_id(),
                    tree.root_hex()
                ),
                chain_id,
                to: claim_contract.as_str().to_string(),
                value: Amount::ZERO,
                data: hex_data(&abi::deposit_calldata(
                    &plan.id,
                    tree.root(),
                    &token,
                    total,
                )?),
            },
        ];

        Ok(Self {
            plan_id: plan.id.clone(),
            merkle_root: tree.root_hex(),
            claim_contract: claim_contract.as_str().to_string(),
            token: Some(token.as_str().to_string()),
            total,
            claims: tree.claims().len(),
            transactions,
        })
    }
}

fn hex_data(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attribution::Attribution;
    use crate::payout::{PlanBuilder, PlanRange};

    fn plan_and_config() -> (PayoutPlan, Config) {
        let mut config = Config::template("demo");
        config.asset.contract = Some("0xdbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB".into());
        config.settlement.chain_id = Some(8453);
        config.settlement.contract = Some("0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359".into());
        // The template ships zero addresses; a round needs real ones.
        config.wallets.treasury =
            Address::parse("0xD1220A0cf47c7B9Be7A2E6BA89F429762e7b9aDb").unwrap();
        config.wallets.open_collective =
            Address::parse("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed").unwrap();

        let plan = PlanBuilder::new(
            &config,
            &Attribution::default(),
            PlanRange {
                branch: "main".into(),
                from_commit: None,
                to_commit: "abc".into(),
                merges: 0,
            },
            Amount::from_base_units(1_000_000),
        )
        .created_at(0)
        .build()
        .unwrap();
        (plan, config)
    }

    #[test]
    fn a_round_proposes_an_approval_then_a_deposit() {
        let (plan, config) = plan_and_config();
        let proposal = RoundProposal::build(&plan, &config).unwrap();

        assert_eq!(proposal.transactions.len(), 2);
        assert_eq!(proposal.transactions[0].step, 1);
        assert_eq!(proposal.transactions[1].step, 2);

        // The approval goes to the token; the deposit to the claim contract.
        assert_eq!(
            proposal.transactions[0].to,
            plan.asset.contract.clone().unwrap()
        );
        assert_eq!(proposal.transactions[1].to, proposal.claim_contract);

        // Nothing carries native coin, and nothing carries a signature.
        assert!(
            proposal
                .transactions
                .iter()
                .all(|t| t.value == Amount::ZERO)
        );
        assert!(
            proposal
                .transactions
                .iter()
                .all(|t| t.data.starts_with("0x"))
        );

        assert!(proposal.merkle_root.starts_with("0x"));
        assert_eq!(proposal.merkle_root.len(), 2 + 64);
    }

    /// The approval must cover exactly what the deposit spends. An approval
    /// for less reverts; one for more leaves an allowance behind.
    #[test]
    fn the_approval_and_the_deposit_name_the_same_total() {
        let (plan, config) = plan_and_config();
        let proposal = RoundProposal::build(&plan, &config).unwrap();

        let approve = hex::decode(&proposal.transactions[0].data[2..]).unwrap();
        let deposit = hex::decode(&proposal.transactions[1].data[2..]).unwrap();

        // approve(address,uint256): the amount is the second word.
        let approved = u128::from_be_bytes(approve[52..68].try_into().unwrap());
        // deposit(bytes16,bytes32,address,uint256): the amount is the fourth.
        let deposited = u128::from_be_bytes(deposit[116..132].try_into().unwrap());

        assert_eq!(approved, deposited);
        assert_eq!(approved, proposal.total.base_units());
    }

    #[test]
    fn a_round_with_no_chain_configured_says_which_setting_is_missing() {
        let (plan, mut config) = plan_and_config();
        config.settlement.chain_id = None;
        let error = RoundProposal::build(&plan, &config)
            .unwrap_err()
            .to_string();
        assert!(error.contains("settlement.chain_id"), "{error}");

        let (plan, mut config) = plan_and_config();
        config.settlement.contract = None;
        let error = RoundProposal::build(&plan, &config)
            .unwrap_err()
            .to_string();
        assert!(error.contains("settlement.contract"), "{error}");
    }

    /// Refusing beats guessing: a native-coin deposit is a different call.
    #[test]
    fn a_native_coin_round_is_refused_rather_than_encoded_wrongly() {
        // Built without a token from the start: editing a finished plan would
        // trip its own id check first, and prove nothing about this path.
        let (_, mut config) = plan_and_config();
        config.asset.contract = None;
        let plan = PlanBuilder::new(
            &config,
            &Attribution::default(),
            PlanRange {
                branch: "main".into(),
                from_commit: None,
                to_commit: "abc".into(),
                merges: 0,
            },
            Amount::from_base_units(1_000_000),
        )
        .created_at(0)
        .build()
        .unwrap();

        let error = RoundProposal::build(&plan, &config)
            .unwrap_err()
            .to_string();
        assert!(error.contains("native-coin rounds"), "{error}");
    }
}
