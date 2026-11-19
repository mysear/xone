//! A server-configured price book is independent of worker output and HTTP
//! request bodies. Apply it only after the parent has audited native fees.
use crate::price_publication::{AuthorizedPublication, Journal};
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
#[derive(serde::Deserialize)]
#[serde(
    tag = "operation",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum PublicationCommand {
    #[serde(skip)]
    Reload,
    Status {},
    Preview {},
    Apply {
        base_entry_hash: String,
        candidate_book_hash: String,
    },
}
impl PublicationCommand {
    pub(crate) fn mutates(&self) -> bool {
        matches!(self, Self::Reload | Self::Apply { .. })
    }
}
pub(crate) struct ValuationState {
    publication: Option<AuthorizedPublication>,
    prices: Vec<AuthorizedPrice>,
    anchor: PriceAnchor,
    execution_timestamp_ms: u64,
    book_hash: String,
    revision: u64,
}

pub(crate) struct ValuationStore {
    // None revokes price admission while a durable commit is in flight or uncertain.
    current: Mutex<Option<Arc<ValuationState>>>,
    journal: Option<Mutex<Journal>>,
    reload: Mutex<()>,
}
impl ValuationStore {
    fn new(state: ValuationState, journal: Option<Journal>) -> Self {
        Self {
            current: Mutex::new(Some(Arc::new(state))),
            journal: journal.map(Mutex::new),
            reload: Mutex::new(()),
        }
    }
    pub(crate) fn load(config: &ConfigSnapshot, fixture: &FixtureState) -> Result<Option<Self>> {
        let Some(state) = ValuationState::load(config, fixture)? else {
            return Ok(None);
        };
        let policy = config
            .config
            .simulation
            .native_valuation
            .as_ref()
            .ok_or_else(|| err("NATIVE_VALUATION_NOT_CONFIGURED"))?;
        let journal = if let Some(path) = &policy.publication_journal {
            let journal = Journal::open(&config.resolve(path), policy)?;
            journal.check(
                state
                    .publication
                    .as_ref()
                    .ok_or_else(|| err("NATIVE_VALUATION_PUBLICATION_REQUIRED"))?,
            )?;
            Some(journal)
        } else {
            None
        };
        Ok(Some(Self::new(state, journal)))
    }
    pub(crate) fn snapshot(&self) -> Result<Arc<ValuationState>> {
        self.current
            .lock()
            .map_err(|_| err("NATIVE_VALUATION_LOCK"))?
            .clone()
            .ok_or_else(|| err("NATIVE_VALUATION_DURABLE_UNAVAILABLE"))
    }
    pub(crate) fn reload(
        &self,
        config: &ConfigSnapshot,
        fixture: &FixtureState,
        command: PublicationCommand,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<Value> {
        let _guard = self
            .reload
            .try_lock()
            .map_err(|_| err("NATIVE_VALUATION_RELOAD_BUSY"))?;
        if !matches!(command, PublicationCommand::Reload) {
            let lock = self
                .journal
                .as_ref()
                .ok_or_else(|| err("NATIVE_VALUATION_JOURNAL_REQUIRED"))?;
            let head = self.snapshot()?;
            let journal = lock
                .lock()
                .map_err(|_| err("NATIVE_VALUATION_DURABLE_UNAVAILABLE"))?;
            let checked = journal.check(
                head.publication
                    .as_ref()
                    .ok_or_else(|| err("NATIVE_VALUATION_PUBLICATION_REQUIRED"))?,
            );
            let receipt = match checked {
                Ok(r) => r,
                Err(e) => {
                    *self
                        .current
                        .lock()
                        .map_err(|_| err("NATIVE_VALUATION_LOCK"))? = None;
                    return Err(e);
                }
            };
            if matches!(command, PublicationCommand::Status {}) {
                return Ok(
                    json!({"priceBookHash": head.book_hash, "publicationRevision":head.revision,
                    "bankAnchor":head.anchor, "bankConsensusAttested":false,
                    "journalEntryHash":receipt.entry_hash, "journalSequence":receipt.sequence,
                    "fsyncConfirmed":receipt.fsync_confirmed, "productionVerified":false}),
                );
            }
            let next = ValuationState::load(config, fixture)?
                .ok_or_else(|| err("NATIVE_VALUATION_NOT_CONFIGURED"))?;
            if let PublicationCommand::Apply {
                base_entry_hash,
                candidate_book_hash,
            } = &command
            {
                if *base_entry_hash != receipt.entry_hash || *candidate_book_hash != next.book_hash
                {
                    return Err(err("NATIVE_VALUATION_PUBLICATION_PRECONDITION"));
                }
            }
            let candidate = next
                .publication
                .as_ref()
                .ok_or_else(|| err("NATIVE_VALUATION_PUBLICATION_REQUIRED"))?;
            let advance = match journal.would_advance(candidate) {
                Err(e) if e.0 == "PRICE_PUBLICATION_JOURNAL_REPLACED" => {
                    *self
                        .current
                        .lock()
                        .map_err(|_| err("NATIVE_VALUATION_LOCK"))? = None;
                    return Err(e);
                }
                other => other?,
            };
            drop(journal);
            if matches!(command, PublicationCommand::Preview {}) {
                return Ok(
                    json!({"wouldAdvance":advance,"published":false,"baseEntryHash":receipt.entry_hash,
                    "bankAnchor":next.anchor, "bankConsensusAttested":false,
                    "candidateBookHash":next.book_hash,"candidateRevision":next.revision,"productionVerified":false}),
                );
            }
            return self.publish(Arc::new(next), cancelled, deadline);
        }
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
        let mut slot = self
            .current
            .lock()
            .map_err(|_| err("NATIVE_VALUATION_LOCK"))?;
        let current = slot
            .as_ref()
            .ok_or_else(|| err("NATIVE_VALUATION_DURABLE_UNAVAILABLE"))?;
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
            if tokens(&next) != tokens(current) {
                return Err(err("NATIVE_VALUATION_PUBLICATION_TOKEN_SET"));
            }
            for price in &next.prices {
                price.validate_at(&next.anchor, next.execution_timestamp_ms)?;
            }
        }
        // Cancellation before commit has no persistent effect. Once a durable
        // write starts, recovery may observe it even without an acknowledgement.
        if cancelled.load(Ordering::Acquire) || Instant::now() >= deadline {
            return Err(err("NATIVE_VALUATION_RELOAD_CANCELLED"));
        }
        if let Some(journal) = &self.journal {
            let publication = next
                .publication
                .as_ref()
                .ok_or_else(|| err("NATIVE_VALUATION_PUBLICATION_REQUIRED"))?;
            let old = slot.take();
            drop(slot);
            let receipt = match journal
                .lock()
                .map_err(|_| err("NATIVE_VALUATION_DURABLE_UNAVAILABLE"))?
                .advance(publication)
            {
                Ok(receipt) => receipt,
                // Capacity is checked before writing; other journal failures require restart/recovery.
                Err(e) if e.0 == "PRICE_PUBLICATION_JOURNAL_LIMIT" => {
                    *self
                        .current
                        .lock()
                        .map_err(|_| err("NATIVE_VALUATION_LOCK"))? = old;
                    return Err(e);
                }
                Err(e) => return Err(e),
            };
            if cancelled.load(Ordering::Acquire) || Instant::now() >= deadline {
                return Err(err("NATIVE_VALUATION_DURABLE_ACK_UNCERTAIN"));
            }
            let response = json!({"changed":receipt.changed,"priceBookHash":next.book_hash,"publicationRevision":next.revision,
                "scope":"LOCAL_DURABLE_FIXED_BANK_AUTHORIZED_PRICE_PUBLICATION","restartRollbackProtected":true,
                "journalSequence":receipt.sequence,"journalEntryHash":receipt.entry_hash,"fsyncConfirmed":receipt.fsync_confirmed,"productionVerified":false});
            *self
                .current
                .lock()
                .map_err(|_| err("NATIVE_VALUATION_LOCK"))? = Some(next);
            return Ok(response);
        }
        let response = json!({"changed":changed,"priceBookHash":next.book_hash,"publicationRevision":next.revision,
            "scope":"PROCESS_LOCAL_FIXED_BANK_AUTHORIZED_PRICE_PUBLICATION",
            "restartRollbackProtected":false,"productionVerified":false});
        if changed {
            *slot = Some(next);
        }
        Ok(response)
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
        let (publication, prices) = if policy.publication_journal.is_some() {
            let (p, prices) = AuthorizedPublication::authorize_with_prices(
                policy,
                &book,
                &anchor,
                execution_timestamp_ms,
            )?;
            (Some(p), prices)
        } else {
            (None, book.authorize(&policy.policies)?)
        };
        let revision = prices[0].publication_revision();
        Ok(Some(Self {
            publication,
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
        let wrapping = response["costs"]["scope"] == "DECLARED_LOCAL_WRAP_AND_SWAP";
        let combined = response["costs"]["scope"] == "DECLARED_LOCAL_WRAP_APPROVAL_AND_SWAP";
        let sequence = combined
            || wrapping
            || response["costs"]["scope"] == "DECLARED_LOCAL_APPROVAL_AND_SWAP";
        let settlement_ms = if sequence {
            response["costs"]["settlementTimestampMs"]
                .as_u64()
                .filter(|at| *at > self.execution_timestamp_ms)
                .ok_or_else(|| err("NATIVE_VALUATION_TIME"))?
        } else {
            self.execution_timestamp_ms
        };
        let price = |token: &str| {
            self.prices.iter().find(|p| p.token() == token).filter(|p| {
                !sequence
                    || p.validate_at(&self.anchor, self.execution_timestamp_ms)
                        .is_ok()
            })
        };
        let mut economics = evaluated_amounts(
            Settlement {
                input: &input,
                output: &output,
                debit: &debit,
                sell_token: &context.sell_token,
                buy_token: &context.buy_token,
                anchor: &self.anchor,
                at_ms: settlement_ms,
            },
            price(&context.sell_token),
            price(&context.buy_token),
        )?;
        if sequence {
            economics["scope"] = if combined {
                "EVALUATED_WRAP_APPROVAL_AND_SWAP_NETWORK_FEES"
            } else if wrapping {
                "EVALUATED_WRAP_AND_SWAP_NETWORK_FEES"
            } else {
                "EVALUATED_APPROVAL_AND_SWAP_NETWORK_FEES"
            }
            .into();
            economics["additionalOperationsIncluded"] = true.into();
        }
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
            publication: None,
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
    fn durable_publication_revokes_old_readers_while_committing_and_after_lost_ack() {
        let v: Value = serde_json::from_str(include_str!(
            "../../../test/contracts/native-valuation-publications.json"
        ))
        .unwrap();
        let policy =
            serde_json::from_value(json!({"observations_file":"prices","policies":v["policies"]}))
                .unwrap();
        let publication = |name: &str| {
            let mut s = state(name).unwrap();
            let book: PriceBook = serde_json::from_value(v["books"][name].clone()).unwrap();
            s.publication = Some(
                AuthorizedPublication::authorize(
                    &policy,
                    &book,
                    &s.anchor,
                    s.execution_timestamp_ms,
                )
                .unwrap(),
            );
            s
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prices.journal");
        let one = publication("1");
        let journal =
            Journal::initialize(&path, &policy, one.publication.as_ref().unwrap()).unwrap();
        let store = ValuationStore::new(one, Some(journal));
        let next = Arc::new(publication("2"));
        let cancelled = AtomicBool::new(false);
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        std::thread::scope(|scope| {
            let hold = store.journal.as_ref().unwrap().lock().unwrap();
            let writer = scope.spawn(|| store.publish(next.clone(), &cancelled, deadline));
            while store.snapshot().is_ok() {
                assert!(Instant::now() < deadline, "commit did not revoke admission");
                std::thread::yield_now();
            }
            cancelled.store(true, Ordering::Release);
            drop(hold);
            assert_eq!(
                writer.join().unwrap().unwrap_err().0,
                "NATIVE_VALUATION_DURABLE_ACK_UNCERTAIN"
            );
        });
        assert!(store.snapshot().is_err());
        assert!(publish(&store, "1").is_err());
        drop(store);
        let journal = Journal::open(&path, &policy).unwrap();
        assert!(journal
            .check(publication("1").publication.as_ref().unwrap())
            .is_err());
        assert_eq!(
            journal
                .check(next.publication.as_ref().unwrap())
                .unwrap()
                .sequence,
            2
        );
    }

    #[test]
    fn signed_publications_are_atomic_monotonic_and_preserve_old_snapshot_arithmetic() {
        let store = ValuationStore::new(state("0").unwrap(), None);
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
        let store = Arc::new(ValuationStore::new(state("1").unwrap(), None));
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
