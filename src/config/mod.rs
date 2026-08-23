//! `dedalo.toml`: the project's funding rules, versioned next to the code.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::attribution::AttributionPolicy;
use crate::attribution::identity::{Identity, IdentityMap};
use crate::chain::wallet::{Address, AddressKind};
use crate::error::{Error, Result};
use crate::money::Asset;
use crate::money::treasury::FeeSchedule;

/// Name of the config file, looked up from the working directory upwards.
pub const CONFIG_FILE: &str = "dedalo.toml";
/// Directory holding the ledger, saved plans and the payout cursor.
pub const STATE_DIR: &str = ".dedalo";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// A project's complete funding policy, as loaded from `dedalo.toml`.
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Who this project is.
    pub project: Project,
    /// Which history counts.
    #[serde(default)]
    pub git: GitConfig,
    /// How merges turn into scores.
    #[serde(default)]
    pub attribution: AttributionPolicy,
    /// The token contributors are paid in.
    pub asset: Asset,
    /// What is taken off the top of every round.
    #[serde(default)]
    pub fees: FeeSchedule,
    /// Where money comes from and goes to.
    pub wallets: Wallets,
    /// How a plan is executed.
    #[serde(default)]
    pub settlement: SettlementConfig,
    /// Contributors, stored as `[[identities]]` tables.
    #[serde(default, rename = "identities")]
    pub identities: Vec<Identity>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// Identity of the project being funded.
#[serde(deny_unknown_fields)]
pub struct Project {
    /// Project name, used in plans and reports.
    pub name: String,
    /// Canonical repository URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// Open Collective slug this project self-funds through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_collective: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// Which slice of git history earns a payout.
#[serde(default, deny_unknown_fields)]
pub struct GitConfig {
    /// Branch whose merges trigger payouts.
    pub branch: String,
    /// Skip merges whose subject matches one of these prefixes.
    pub ignore_subjects: Vec<String>,
    /// Never pay these emails (bots, CI accounts).
    pub ignore_emails: Vec<String>,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            branch: "main".into(),
            ignore_subjects: Vec::new(),
            ignore_emails: vec!["noreply@github.com".into(), "actions@github.com".into()],
        }
    }
}

/// Where money goes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wallets {
    /// Funds the round is paid out of.
    pub source: Address,
    /// Long-term project reserve.
    pub treasury: Address,
    /// Open Collective wallet that receives the protocol fee. This is how the
    /// network sustains itself and funds the projects on it.
    pub open_collective: Address,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// How a verified plan reaches a chain.
#[serde(default, deny_unknown_fields)]
pub struct SettlementConfig {
    /// Backend id: `dry-run` (default) or `evm`.
    pub backend: String,
    /// JSON-RPC endpoint of the chain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpc_url: Option<String>,
    /// EIP-155 chain id, checked against the endpoint before signing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_id: Option<u64>,
    /// Claim contract a round is deposited into.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract: Option<String>,
}

impl Default for SettlementConfig {
    fn default() -> Self {
        Self {
            backend: "dry-run".into(),
            rpc_url: None,
            chain_id: None,
            contract: None,
        }
    }
}

impl Config {
    /// A ready-to-edit config for a freshly initialised project.
    pub fn template(name: &str) -> Self {
        Self {
            project: Project {
                name: name.to_string(),
                repository: None,
                open_collective: None,
            },
            git: GitConfig::default(),
            attribution: AttributionPolicy::default(),
            asset: Asset {
                symbol: "USDC".into(),
                decimals: 6,
                chain: "base".into(),
                contract: Some("0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913".into()),
            },
            fees: FeeSchedule::default(),
            wallets: Wallets {
                source: Address::parse(crate::chain::wallet::ZERO_ADDRESS)
                    .expect("the zero address is valid"),
                treasury: Address::parse(crate::chain::wallet::ZERO_ADDRESS)
                    .expect("the zero address is valid"),
                open_collective: Address::parse(crate::chain::wallet::ZERO_ADDRESS)
                    .expect("the zero address is valid"),
            },
            settlement: SettlementConfig::default(),
            identities: Vec::new(),
        }
    }

    /// Walk up from `start` looking for `dedalo.toml`.
    pub fn discover(start: impl AsRef<Path>) -> Result<(Self, PathBuf)> {
        let start = start.as_ref();
        let mut current = Some(start);
        while let Some(dir) = current {
            let candidate = dir.join(CONFIG_FILE);
            if candidate.is_file() {
                let config = Self::load(&candidate)?;
                return Ok((config, candidate));
            }
            current = dir.parent();
        }
        Err(Error::ConfigNotFound(start.to_path_buf()))
    }

    /// Read and validate a config from an exact path.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        let config: Config = toml::from_str(&raw).map_err(|source| Error::ConfigParse {
            path: path.to_path_buf(),
            source,
        })?;
        config.validate()?;
        Ok(config)
    }

    /// Serialize this config back to TOML. Comments are not preserved.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let raw = toml::to_string_pretty(self)
            .map_err(|e| Error::config(format!("cannot serialize config: {e}")))?;
        std::fs::write(path, raw).map_err(|e| Error::io(path, e))
    }

    /// The contributor lookup built from `[[identities]]`.
    pub fn identity_map(&self) -> IdentityMap {
        IdentityMap::new(self.identities.clone())
    }

    /// Reject configs that would produce nonsensical or unpayable rounds.
    pub fn validate(&self) -> Result<()> {
        if self.project.name.trim().is_empty() {
            return Err(Error::config("project.name must not be empty"));
        }
        if self.asset.decimals > 38 {
            return Err(Error::config("asset.decimals is unrealistically large"));
        }
        self.fees.validate()?;
        self.attribution.validate()?;
        if self.git.branch.trim().is_empty() {
            return Err(Error::config("git.branch must not be empty"));
        }
        // An address that is well-formed for the wrong chain is still an
        // address the funds cannot reach. Cross-check it against the chain the
        // asset actually lives on.
        if let Some(expected) = AddressKind::for_chain(&self.asset.chain) {
            let check = |label: &str, address: &Address| -> Result<()> {
                if address.kind() != expected {
                    return Err(Error::config(format!(
                        "{label} is a {:?} address, but asset.chain is `{}`, which expects {}",
                        address.kind(),
                        self.asset.chain,
                        expected.description()
                    )));
                }
                Ok(())
            };
            check("wallets.source", &self.wallets.source)?;
            check("wallets.treasury", &self.wallets.treasury)?;
            check("wallets.open_collective", &self.wallets.open_collective)?;
            for identity in &self.identities {
                if let Some(wallet) = &identity.wallet {
                    check(&format!("identity `{}`", identity.handle), wallet)?;
                }
            }
        }

        for identity in &self.identities {
            if identity.handle.trim().is_empty() {
                return Err(Error::config("every identity needs a handle"));
            }
            if identity.wallet.is_none() && !identity.excluded {
                return Err(Error::config(format!(
                    "identity `{}` has no wallet; set one or mark it excluded",
                    identity.handle
                )));
            }
        }
        Ok(())
    }

    /// Should this author's email be skipped entirely?
    pub fn is_ignored_email(&self, email: &str) -> bool {
        let email = email.trim().to_ascii_lowercase();
        self.git
            .ignore_emails
            .iter()
            .any(|ignored| ignored.trim().eq_ignore_ascii_case(&email))
    }

    /// Should this merge be skipped based on its subject line?
    pub fn is_ignored_subject(&self, subject: &str) -> bool {
        self.git
            .ignore_subjects
            .iter()
            .any(|prefix| subject.starts_with(prefix.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_round_trips_through_toml() {
        let config = Config::template("dedalo");
        let raw = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&raw).unwrap();
        assert_eq!(config, parsed);
        parsed.validate().unwrap();
    }

    #[test]
    fn rejects_identities_without_wallets() {
        let mut config = Config::template("dedalo");
        config.identities.push(Identity {
            handle: "ada".into(),
            wallet: None,
            emails: vec!["ada@example.com".into()],
            excluded: false,
        });
        assert!(config.validate().is_err());
    }
}
