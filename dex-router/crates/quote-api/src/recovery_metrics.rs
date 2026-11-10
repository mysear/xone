use super::{Api, QuoteView};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fmt::Write, sync::Mutex};

#[derive(Clone, Default)]
pub(crate) struct ScopeRecoveryTiming {
    first_snapshot_ms: Option<u64>,
    first_quote_ms: Option<u64>,
    first_quote_generation: Option<u64>,
    latest_generation: u64,
    last_qualified_pools: usize,
}
pub(crate) type RecoveryTimings = Mutex<BTreeMap<String, ScopeRecoveryTiming>>;
impl Api {
    fn recovery_elapsed_ms(&self) -> u64 {
        self.constructed_at
            .elapsed()
            .as_millis()
            .min(u64::MAX as u128) as u64
    }
    pub(crate) fn observe_qualified_view(&self, view: &QuoteView) {
        let eligible = view
            .pools
            .iter()
            .filter(|pool| !view.graph.excluded_pools.contains_key(&pool.pool_id))
            .count();
        if eligible == 0 {
            return;
        }
        if let Ok(mut all) = self.metrics.recovery.lock() {
            let timing = all
                .entry(view.reference.block.chain_key.clone())
                .or_default();
            timing
                .first_snapshot_ms
                .get_or_insert(self.recovery_elapsed_ms());
            if view.reference.online_generation >= timing.latest_generation {
                timing.latest_generation = view.reference.online_generation;
                timing.last_qualified_pools = eligible;
            }
        }
    }
    pub(crate) fn observe_successful_quote(&self, view: &QuoteView) {
        self.observe_qualified_view(view);
        if let Ok(mut all) = self.metrics.recovery.lock() {
            if let Some(timing) = all.get_mut(&view.reference.block.chain_key) {
                if timing.first_quote_ms.is_none() {
                    timing.first_quote_ms = Some(self.recovery_elapsed_ms());
                    timing.first_quote_generation = Some(view.reference.online_generation);
                }
            }
        }
    }
    pub(crate) fn observe_ready_views(&self) {
        for chain in &self.config.config.chains {
            if let Ok(view) = self.replica.view(&chain.key) {
                // This is the normal freshness/epoch/view check, not a claim
                // that any route or simulation has successfully executed.
                let _ = self.check_view(&view);
            }
        }
    }
    pub(crate) fn recovery_status(&self, chain: &str) -> Value {
        self.metrics.recovery.lock().ok().and_then(|all| all.get(chain).cloned()).map_or_else(
            || json!({"observed":false}),
            |timing| json!({"observed":true,"first_qualified_snapshot_ms":timing.first_snapshot_ms,"first_successful_quote_ms":timing.first_quote_ms,"first_quote_generation":timing.first_quote_generation,"online_generation":timing.latest_generation,"last_qualified_pool_count":timing.last_qualified_pools,"history_blocks_readiness":false,"timing_origin":"quote_api_created"}),
        )
    }
    pub(crate) fn recovery_prometheus(&self) -> String {
        let mut result = String::new();
        if let Ok(all) = self.metrics.recovery.lock() {
            for (chain, timing) in all.iter() {
                if let Some(ms) = timing.first_snapshot_ms {
                    let _ = writeln!(
                        result,
                        "xone_first_qualified_snapshot_seconds{{chain={chain:?}}} {:.3}",
                        ms as f64 / 1000.0
                    );
                }
                if let Some(ms) = timing.first_quote_ms {
                    let _ = writeln!(
                        result,
                        "xone_first_successful_quote_seconds{{chain={chain:?}}} {:.3}",
                        ms as f64 / 1000.0
                    );
                }
                let _ = writeln!(
                    result,
                    "xone_last_qualified_scope_pool_count{{chain={chain:?}}} {}",
                    timing.last_qualified_pools
                );
            }
        }
        result
    }
}
