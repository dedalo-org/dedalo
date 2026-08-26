//! The default backend: verifies and prices a round without spending anything.
//!
//! This is what runs in CI on every pull request, so a payout can be reviewed
//! before it is real.

use async_trait::async_trait;

use super::{Settlement, SettlementReceipt};
use crate::error::Result;
use crate::money::{Amount, Asset};
use crate::payout::{PayoutPlan, now_unix};

/// A backend that verifies and prices a round without spending anything.
#[derive(Debug, Clone, Default)]
pub struct DryRunSettlement {
    /// Optional simulated balance, used to exercise insufficient-funds paths.
    pub balance: Option<Amount>,
}

impl DryRunSettlement {
    /// A simulator that reports a specific balance.
    pub fn with_balance(balance: Amount) -> Self {
        Self {
            balance: Some(balance),
        }
    }
}

#[async_trait]
impl Settlement for DryRunSettlement {
    fn name(&self) -> &str {
        "dry-run"
    }

    fn is_dry_run(&self) -> bool {
        true
    }

    async fn balance(&self, _asset: &Asset) -> Result<Option<Amount>> {
        Ok(self.balance)
    }

    async fn settle(&self, plan: &PayoutPlan) -> Result<SettlementReceipt> {
        plan.verify()?;
        let total = plan.total()?;
        Ok(SettlementReceipt {
            plan_id: plan.id.clone(),
            backend: self.name().to_string(),
            at: now_unix(),
            tx: None,
            total,
            transfers: plan.payable_items().count(),
            dry_run: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attribution::Attribution;
    use crate::config::Config;
    use crate::payout::{PlanBuilder, PlanRange};

    #[test]
    fn simulates_without_touching_a_chain() {
        let config = Config::template("dedalo");
        let attribution = Attribution::default();
        let plan = PlanBuilder::new(
            &config,
            &attribution,
            PlanRange {
                branch: "main".into(),
                from_commit: None,
                to_commit: "abc".into(),
                merges: 0,
            },
            Amount::from_base_units(1_000),
        )
        .created_at(0)
        .build()
        .unwrap();

        let receipt = futures_lite_block_on(DryRunSettlement::default().settle(&plan));
        let receipt = receipt.unwrap();
        assert!(receipt.dry_run);
        assert!(receipt.tx.is_none());
    }

    /// Minimal executor: the core crate stays runtime-agnostic, so tests
    /// poll the future by hand instead of pulling in tokio.
    fn futures_lite_block_on<T>(future: impl Future<Output = T>) -> T {
        use std::pin::pin;
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

        const VTABLE: RawWakerVTable = RawWakerVTable::new(
            |_| RawWaker::new(std::ptr::null(), &VTABLE),
            |_| {},
            |_| {},
            |_| {},
        );
        let waker = unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) };
        let mut cx = Context::from_waker(&waker);
        let mut future = pin!(future);
        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(value) => return value,
                // Our futures never actually yield; a pending poll would spin.
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }
}
