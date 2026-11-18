//! A server-configured price book is independent of worker output and HTTP
//! request bodies. Apply it only after the parent has audited native fees.
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    io::Read,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};
use xone_config::ConfigSnapshot;
use xone_domain::{err, hash, Amount, Result};
use xone_execution_plan::Context;
use xone_execution_preparation::native_valuation::{
    evaluated_amounts, AuthorizedPrice, PriceAnchor, PriceBook, Settlement,
};
use xone_simulation::evm_fixture::FixtureState;

const MAX_PRICE_BYTES: u64 = 1024 * 1024;
pub(crate) struct ValuationState {
    prices: Vec<AuthorizedPrice>,
    anchor: PriceAnchor,
    execution_timestamp_ms: u64,
    book_hash: String,
    revision: u64,
}

pub(crate) struct ValuationStore {
    current: Mutex<Arc<ValuationState>>,
    reload: Mutex<()>,
}
impl ValuationStore {
    pub(crate) fn new(state: ValuationState) -> Self {
        Self {
            current: Mutex::new(Arc::new(state)),
            reload: Mutex::new(()),
        }
    }
    pub(crate) fn snapshot(&self) -> Result<Arc<ValuationState>> {
        self.current
            .lock()
            .map(|state| state.clone())
            .map_err(|_| err("NATIVE_VALUATION_LOCK"))
    }
    pub(crate) fn reload(
        &self,
        config: &ConfigSnapshot,
        fixture: &FixtureState,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<Value> {
        let _guard = self
            .reload
            .try_lock()
            .map_err(|_| err("NATIVE_VALUATION_RELOAD_BUSY"))?;
        let state = ValuationState::load(config, fixture)?
            .ok_or_else(|| err("NATIVE_VALUATION_NOT_CONFIGURED"))?;
        self.publish(Arc::new(state), cancelled, deadline)
    }
    fn publish(
        &self,
        next: Arc<ValuationState>,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<Value> {
        let mut current = self
            .current
            .lock()
            .map_err(|_| err("NATIVE_VALUATION_LOCK"))?;
        let changed = current.book_hash != next.book_hash;
        if changed {
            if next.revision == 0 || next.revision <= current.revision {
                return Err(err("NATIVE_VALUATION_REVISION_ROLLBACK_OR_CONFLICT"));
            }
            if next.anchor != current.anchor
                || next.execution_timestamp_ms != current.execution_timestamp_ms
            {
                return Err(err("NATIVE_VALUATION_ANCHOR"));
            }
            let tokens = |state: &ValuationState| {
                state
                    .prices
                    .iter()
                    .map(|p| p.token().to_owned())
                    .collect::<BTreeSet<_>>()
            };
            if tokens(&next) != tokens(&current) {
                return Err(err("NATIVE_VALUATION_PUBLICATION_TOKEN_SET"));
            }
            for price in &next.prices {
                price.validate_at(&next.anchor, next.execution_timestamp_ms)?;
            }
        }
        // A timed-out handler must not publish later from its owned blocking task.
        if cancelled.load(Ordering::Acquire) || Instant::now() >= deadline {
            return Err(err("NATIVE_VALUATION_RELOAD_CANCELLED"));
        }
        if changed {
            *current = next;
        }
        Ok(
            json!({"changed":changed,"priceBookHash":current.book_hash,"publicationRevision":current.revision,
            "scope":"PROCESS_LOCAL_FIXED_BANK_AUTHORIZED_PRICE_PUBLICATION",
            "restartRollbackProtected":false,"productionVerified":false}),
        )
    }
}
impl ValuationState {
    pub(crate) fn book_hash(&self) -> &str {
        &self.book_hash
    }
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
    pub(crate) fn load(config: &ConfigSnapshot, fixture: &FixtureState) -> Result<Option<Self>> {
        let Some(policy) = &config.config.simulation.native_valuation else {
            return Ok(None);
        };
        policy.validate()?;
        let path = config.resolve(&policy.observations_file);
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        // Inspect the opened handle, but do not block opening a FIFO while
        // waiting for a writer. Only regular handles may be read below.
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = options
            .open(path)
            .map_err(|_| err("NATIVE_VALUATION_BOOK_UNREADABLE"))?;
        let metadata = file
            .metadata()
            .map_err(|_| err("NATIVE_VALUATION_BOOK_UNREADABLE"))?;
        if !metadata.is_file() || metadata.len() > MAX_PRICE_BYTES {
            return Err(err("NATIVE_VALUATION_BOOK_LIMIT"));
        }
        let mut bytes = Vec::new();
        file.take(MAX_PRICE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| err("NATIVE_VALUATION_BOOK_UNREADABLE"))?;
        if bytes.len() as u64 > MAX_PRICE_BYTES {
            return Err(err("NATIVE_VALUATION_BOOK_LIMIT"));
        }
        let book: PriceBook =
            serde_json::from_slice(&bytes).map_err(|_| err("NATIVE_VALUATION_BOOK_INVALID"))?;
        let source = &fixture.state_anchor;
        let anchor = PriceAnchor {
            chain_key: source.chain_key.clone(),
            genesis_hash: source.genesis_hash.clone(),
            block_number: source.block_number,
            block_hash: source.block_hash.clone(),
            parent_hash: source.parent_hash.clone(),
            timestamp_ms: source
                .timestamp
                .checked_mul(1000)
                .ok_or_else(|| err("NATIVE_VALUATION_TIME"))?,
        };
        if book.prices.iter().any(|p| p.observation.anchor != anchor) {
            return Err(err("NATIVE_VALUATION_ANCHOR"));
        }
        let execution_timestamp_ms = fixture
            .execution_environment
            .timestamp
            .checked_mul(1000)
            .ok_or_else(|| err("NATIVE_VALUATION_TIME"))?;
        if execution_timestamp_ms < anchor.timestamp_ms {
            return Err(err("NATIVE_VALUATION_TIME"));
        }
        let prices = book.authorize(&policy.policies)?;
        let revision = prices[0].publication_revision();
        Ok(Some(Self {
            prices,
            anchor,
            execution_timestamp_ms,
            book_hash: hash(&book)?,
            revision,
        }))
    }
    pub(crate) fn apply(&self, context: &Context, response: &mut Value) -> Result<()> {
        if context.chain_key != self.anchor.chain_key {
            return Err(err("NATIVE_VALUATION_ANCHOR"));
        }
        if response["verification"]["success"] != true {
            response["costs"]["transactionEconomics"] = json!({"scope":"EVALUATED_TRANSACTION_NETWORK_FEES_ONLY","status":"EXECUTION_NOT_SUCCESSFUL","quoteRankingApplied":false,"productionVerified":false});
            return Ok(());
        }
        if response["costs"]["balanceReconciliation"]["status"] != "MATCHED"
            || response["costs"]["nativeUnit"] != "EVM_NATIVE_ATOMS"
        {
            return Err(err("NATIVE_VALUATION_FEE_AUDIT"));
        }
        let number = |v: &Value| -> Result<Amount> {
            v.as_str()
                .ok_or_else(|| err("NATIVE_VALUATION_FEE_AUDIT"))?
                .parse()
        };
        let input = number(&response["outcome"]["actual_input_consumed"])?;
        let output = number(&response["outcome"]["actual_buy_amount"])?;
        let debit = number(&response["costs"]["payerNetNativeDebit"])?;
        let price = |token: &str| self.prices.iter().find(|p| p.token() == token);
        let mut economics = evaluated_amounts(
            Settlement {
                input: &input,
                output: &output,
                debit: &debit,
                sell_token: &context.sell_token,
                buy_token: &context.buy_token,
                anchor: &self.anchor,
                at_ms: self.execution_timestamp_ms,
            },
            price(&context.sell_token),
            price(&context.buy_token),
        )?;
        economics["priceBookHash"] = self.book_hash.clone().into();
        if self.revision != 0 {
            economics["publicationRevision"] = self.revision.into();
        }
        economics["bindingHash"] = response["bindingHash"].clone();
        economics["feePayer"] = response["transaction"]["from"].clone();
        economics["recipient"] = context.recipient.clone().into();
        economics["grossChainFeeNative"] = response["costs"]["nativeKnownSubtotal"].clone();
        economics["bankAuthority"] = "PROVIDED_LOCAL_STATE".into();
        response["costs"]["transactionEconomics"] = economics;
        Ok(())
    }
}

#[cfg(test)]
mod publication_tests {
    use super::*;

    fn state(name: &str) -> Result<ValuationState> {
        let v: Value = serde_json::from_str(include_str!(
            "../../../test/contracts/native-valuation-publications.json"
        ))
        .unwrap();
        let book: PriceBook = serde_json::from_value(v["books"][name].clone()).unwrap();
        let policies: Vec<xone_domain::native_valuation::PricePolicy> =
            serde_json::from_value(v["policies"].clone()).unwrap();
        let prices = book.authorize(&policies)?;
        let anchor: PriceAnchor = serde_json::from_value(v["anchor"].clone()).unwrap();
        Ok(ValuationState {
            revision: prices[0].publication_revision(),
            book_hash: hash(&book)?,
            execution_timestamp_ms: anchor.timestamp_ms,
            anchor,
            prices,
        })
    }
    fn publish(store: &ValuationStore, name: &str) -> Result<Value> {
        store.publish(
            Arc::new(state(name)?),
            &AtomicBool::new(false),
            Instant::now() + std::time::Duration::from_secs(1),
        )
    }

    #[test]
    fn signed_publications_are_atomic_monotonic_and_preserve_old_snapshot_arithmetic() {
        let store = ValuationStore::new(state("0").unwrap());
        let old = store.snapshot().unwrap();
        let settlement = |price: &ValuationState| {
            price.prices[0]
                .convert_cost(
                    &"3".parse().unwrap(),
                    &price.anchor,
                    price.execution_timestamp_ms,
                )
                .unwrap()
                .to_string()
        };
        assert_eq!(settlement(&old), "1");
        assert_eq!(publish(&store, "0").unwrap()["changed"], false);
        assert_eq!(publish(&store, "1").unwrap()["publicationRevision"], 1);
        assert_eq!(publish(&store, "2").unwrap()["publicationRevision"], 2);
        assert_eq!(settlement(&store.snapshot().unwrap()), "3");
        assert_eq!(settlement(&old), "1");
        assert_eq!(old.revision(), 0);
        assert_ne!(old.book_hash(), store.snapshot().unwrap().book_hash());
        assert_eq!(publish(&store, "2").unwrap()["changed"], false);
        for name in [
            "0", "1", "conflict", "missing", "stale", "future", "mixed", "tampered",
        ] {
            let before = store.snapshot().unwrap();
            assert!(publish(&store, name).is_err(), "{name}");
            assert!(Arc::ptr_eq(&before, &store.snapshot().unwrap()), "{name}");
        }
        assert_eq!(publish(&store, "3").unwrap()["publicationRevision"], 3);
    }

    #[test]
    fn publication_cancellation_deadline_and_concurrent_readers_keep_one_complete_book() {
        let store = Arc::new(ValuationStore::new(state("1").unwrap()));
        let before = store.snapshot().unwrap();
        for (cancelled, deadline) in [
            (true, Instant::now() + std::time::Duration::from_secs(1)),
            (false, Instant::now()),
        ] {
            assert!(store
                .publish(
                    Arc::new(state("2").unwrap()),
                    &AtomicBool::new(cancelled),
                    deadline
                )
                .is_err());
            assert!(Arc::ptr_eq(&before, &store.snapshot().unwrap()));
        }
        let barrier = Arc::new(std::sync::Barrier::new(5));
        let handles = (0..4)
            .map(|_| {
                let store = store.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    for _ in 0..1000 {
                        let p = store.snapshot().unwrap();
                        assert!([1, 2].contains(&p.revision));
                        assert_eq!(p.prices.len(), 2);
                        assert!(p
                            .prices
                            .iter()
                            .all(|price| price.publication_revision() == p.revision));
                        assert_eq!(
                            p.prices[0]
                                .convert_cost(
                                    &"3".parse().unwrap(),
                                    &p.anchor,
                                    p.execution_timestamp_ms
                                )
                                .unwrap()
                                .to_string(),
                            (p.revision + 1).to_string()
                        );
                    }
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        publish(&store, "2").unwrap();
        for handle in handles {
            handle.join().unwrap();
        }
    }
}
