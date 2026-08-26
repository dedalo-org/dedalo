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
use crate::chain::settlement::instruction;
use crate::chain::wallet::Address;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::money::Amount;
use crate::payout::PayoutPlan;

/// One account an instruction touches, as a signer must check it.
///
/// Solana instructions carry their accounts explicitly, and *which* accounts
/// an instruction is given is as much a part of what it does as its data. A
/// signer who checks only the data has checked half of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountRef {
    /// What this account is for, in words.
    pub role: String,
    /// The address, where it is known ahead of time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// How to derive it, for a program-derived address.
    ///
    /// Present instead of `address` rather than as well as it. Deriving these
    /// requires the claim program's id and its seed layout, and **the claim
    /// program does not exist yet** — so this states the seeds the program
    /// must use and leaves the arithmetic to whoever builds the transaction.
    /// Printing a computed address for a program nobody has written would be
    /// inventing on-chain behaviour, which is the one thing this crate must
    /// never do.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derivation: Option<String>,
    /// Whether this account must sign.
    pub signer: bool,
    /// Whether the instruction may modify it.
    pub writable: bool,
}

/// One instruction for a signer to execute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstructionProposal {
    /// Position in the sequence. They are not independent: a deposit before
    /// its approval fails.
    pub step: u32,
    /// What this instruction does, for the person deciding whether to sign.
    pub description: String,
    /// Cluster this must be executed on. A signer with several configured
    /// needs to be told which, not left to guess.
    pub cluster: String,
    /// Program that executes it.
    pub program_id: String,
    /// Every account the instruction takes, in order. Order is part of the
    /// interface: Anchor matches them positionally.
    pub accounts: Vec<AccountRef>,
    /// Instruction data, hex.
    ///
    /// Hex rather than the base58 an explorer shows, because what a signer
    /// compares this against is a plan id and a Merkle root, and both of those
    /// are already hex. Making them the same alphabet is the difference
    /// between checking and squinting.
    pub data: String,
}

/// Everything a signer needs to fund one round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundProposal {
    /// The plan being funded. Passed on chain, so a round can be paid once.
    pub plan_id: String,
    /// Root of the tree contributors prove against, `0x`-prefixed.
    pub merkle_root: String,
    /// Program that holds the deposit and pays claims.
    pub claim_program: String,
    /// Token being distributed, or `None` for the chain's native coin.
    pub token: Option<String>,
    /// Sum of every claim. What the deposit must cover exactly.
    pub total: Amount,
    /// How many contributors can claim.
    pub claims: usize,
    /// The instructions, in the order they must run.
    pub instructions: Vec<InstructionProposal>,
}

impl RoundProposal {
    /// Build the proposal for a plan, from the project's settlement config.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] if the settlement section does not name a
    /// cluster and a claim program, and [`Error::Config`] if the plan pays
    /// nobody — there is nothing to deposit against.
    ///
    /// Returns [`Error::NotImplemented`] for a round in native SOL: wrapping
    /// and unwrapping SOL is a different sequence, and writing it untested
    /// would be guessing at how money moves.
    pub fn build(plan: &PayoutPlan, config: &Config) -> Result<Self> {
        plan.verify()?;

        let settlement = &config.settlement;
        let cluster = settlement
            .cluster
            .clone()
            .ok_or_else(|| Error::config("settlement.cluster is required to propose a round"))?;
        let claim_program = settlement
            .program_id
            .as_deref()
            .ok_or_else(|| Error::config("settlement.program_id is required to propose a round"))?;
        let claim_program = Address::parse(claim_program)?;

        let mint = match plan.asset.contract.as_deref() {
            Some(contract) => Address::parse(contract)?,
            None => {
                return Err(Error::NotImplemented {
                    feature: "rounds in native SOL",
                    hint: "set asset.contract to an SPL mint; paying in SOL means wrapping \
                           and unwrapping it, which is a different sequence and is not \
                           written yet",
                });
            }
        };

        let tree = ClaimTree::from_plan(plan)?;
        let total = tree.total()?;

        let instructions = vec![
            // Step one is SPL Token's own `Approve`, whose account order is
            // published and stable: source, delegate, owner. Nothing here is
            // this project's invention.
            InstructionProposal {
                step: 1,
                description: format!(
                    "delegate {} {} to the round vault",
                    plan.asset.format_amount(total),
                    plan.asset.symbol
                ),
                cluster: cluster.clone(),
                program_id: SPL_TOKEN_PROGRAM.to_string(),
                accounts: vec![
                    AccountRef {
                        role: "source token account, held by the multisig".into(),
                        address: None,
                        derivation: Some(format!(
                            "associated token account of the signing multisig for mint {}",
                            mint.as_str()
                        )),
                        signer: false,
                        writable: true,
                    },
                    AccountRef {
                        role: "delegate: the round vault".into(),
                        address: None,
                        derivation: Some(format!(
                            "PDA of {} with seeds [\"round\", plan_id]",
                            claim_program.as_str()
                        )),
                        signer: false,
                        writable: false,
                    },
                    AccountRef {
                        role: "owner of the source account".into(),
                        address: None,
                        derivation: Some("the signing multisig".into()),
                        signer: true,
                        writable: false,
                    },
                ],
                data: hex::encode(instruction::approve_data(total)?),
            },
            InstructionProposal {
                step: 2,
                description: format!(
                    "deposit round {} against root {}",
                    plan.short_id(),
                    tree.root_hex()
                ),
                cluster: cluster.clone(),
                program_id: claim_program.as_str().to_string(),
                accounts: vec![
                    AccountRef {
                        role: "round record".into(),
                        address: None,
                        derivation: Some(format!(
                            "PDA of {} with seeds [\"round\", plan_id]",
                            claim_program.as_str()
                        )),
                        signer: false,
                        writable: true,
                    },
                    AccountRef {
                        role: "token mint being distributed".into(),
                        address: Some(mint.as_str().to_string()),
                        derivation: None,
                        signer: false,
                        writable: false,
                    },
                    AccountRef {
                        role: "authority funding the round".into(),
                        address: None,
                        derivation: Some("the signing multisig".into()),
                        signer: true,
                        writable: true,
                    },
                    AccountRef {
                        role: "SPL Token program".into(),
                        address: Some(SPL_TOKEN_PROGRAM.to_string()),
                        derivation: None,
                        signer: false,
                        writable: false,
                    },
                ],
                data: hex::encode(instruction::deposit_data(&plan.id, tree.root(), total)?),
            },
        ];

        Ok(Self {
            plan_id: plan.id.clone(),
            merkle_root: tree.root_hex(),
            claim_program: claim_program.as_str().to_string(),
            token: Some(mint.as_str().to_string()),
            total,
            claims: tree.claims().len(),
            instructions,
        })
    }
}

/// SPL Token, whose address is the same on every cluster.
const SPL_TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attribution::Attribution;
    use crate::payout::{PlanBuilder, PlanRange};

    fn plan_and_config() -> (PayoutPlan, Config) {
        let mut config = Config::template("demo");
        config.asset.contract = Some("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU".into());
        config.settlement.cluster = Some("devnet".into());
        config.settlement.program_id = Some("MerkS3LaQBSvM5JZsvBaLZBBSMvMB5aTuLRHrvKAyDo".into());
        // The template ships zero addresses; a round needs real ones.
        config.wallets.treasury =
            Address::parse("So11111111111111111111111111111111111111112").unwrap();
        config.wallets.open_collective =
            Address::parse("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL").unwrap();

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

        assert_eq!(proposal.instructions.len(), 2);
        assert_eq!(proposal.instructions[0].step, 1);
        assert_eq!(proposal.instructions[1].step, 2);

        // The approval is the token program's; the deposit is the claim
        // program's. Two different programs, which is the thing a signer most
        // needs to see and the EVM version could not express.
        assert_eq!(proposal.instructions[0].program_id, SPL_TOKEN_PROGRAM);
        assert_eq!(proposal.instructions[1].program_id, proposal.claim_program);

        // Both name the cluster, so a signer is never left inferring it.
        assert!(proposal.instructions.iter().all(|i| i.cluster == "devnet"));

        assert!(proposal.merkle_root.starts_with("0x"));
        assert_eq!(proposal.merkle_root.len(), 2 + 64);
    }

    /// Every account is either known or explained. A blank in this list is a
    /// thing a signer would have to invent, and inventing an account is how a
    /// deposit lands somewhere nobody meant.
    #[test]
    fn every_account_is_either_named_or_derived() {
        let (plan, config) = plan_and_config();
        let proposal = RoundProposal::build(&plan, &config).unwrap();

        for instruction in &proposal.instructions {
            assert!(!instruction.accounts.is_empty());
            for account in &instruction.accounts {
                assert!(
                    account.address.is_some() != account.derivation.is_some(),
                    "{} is neither named nor derived, or claims to be both",
                    account.role
                );
                assert!(!account.role.trim().is_empty());
            }

            // Exactly one signer per instruction: the multisig funding it.
            let signers = instruction.accounts.iter().filter(|a| a.signer).count();
            assert_eq!(signers, 1, "step {}", instruction.step);
        }
    }

    /// The approval must cover exactly what the deposit spends. An approval
    /// for less fails; one for more leaves a delegation behind.
    #[test]
    fn the_approval_and_the_deposit_name_the_same_total() {
        let (plan, config) = plan_and_config();
        let proposal = RoundProposal::build(&plan, &config).unwrap();

        let approve = hex::decode(&proposal.instructions[0].data).unwrap();
        let deposit = hex::decode(&proposal.instructions[1].data).unwrap();

        // Approve: a tag byte, then the amount.
        let approved = u64::from_le_bytes(approve[1..9].try_into().unwrap());
        // deposit: discriminator, plan id, root, then the amount.
        let deposited = u64::from_le_bytes(deposit[56..64].try_into().unwrap());

        assert_eq!(approved, deposited);
        assert_eq!(u128::from(approved), proposal.total.base_units());
    }

    #[test]
    fn a_round_with_no_chain_configured_says_which_setting_is_missing() {
        let (plan, mut config) = plan_and_config();
        config.settlement.cluster = None;
        let error = RoundProposal::build(&plan, &config)
            .unwrap_err()
            .to_string();
        assert!(error.contains("settlement.cluster"), "{error}");

        let (plan, mut config) = plan_and_config();
        config.settlement.program_id = None;
        let error = RoundProposal::build(&plan, &config)
            .unwrap_err()
            .to_string();
        assert!(error.contains("settlement.program_id"), "{error}");
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
        assert!(error.contains("native SOL"), "{error}");
    }
}
