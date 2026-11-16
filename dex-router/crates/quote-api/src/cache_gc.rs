//! Periodic removal of expired/revoked cache owners releases old QuoteViews even
//! when the service receives no further quote or preparation requests.
use super::*;

/// The server's source-sync and cache workers must also stop if run() is
/// cancelled by an embedding application before graceful shutdown completes.
pub(crate) struct BackgroundTasks(pub(crate) Vec<tokio::task::JoinHandle<()>>);
impl BackgroundTasks {
    pub(crate) async fn stop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
        while let Some(task) = self.0.pop() {
            let _ = task.await;
        }
    }
}
impl Drop for BackgroundTasks {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Pruned {
    pub exact_in: usize,
    pub exact_out: usize,
    pub preparations: usize,
}
impl Pruned {
    fn total(&self) -> usize {
        self.exact_in + self.exact_out + self.preparations
    }
}
fn reclaim<T>(cache: &Mutex<BTreeMap<String, T>>, keep: impl Fn(&T) -> bool) -> Result<usize> {
    let mut entries = cache.lock().map_err(|_| err("CACHE_LOCK"))?;
    let expired: Vec<_> = entries
        .iter()
        .filter(|(_, value)| !keep(value))
        .map(|(key, _)| key.clone())
        .collect();
    let retired: Vec<_> = expired
        .iter()
        .filter_map(|key| entries.remove(key))
        .collect();
    drop(entries);
    let count = retired.len();
    // A final Arc may own a large snapshot; destroy it outside the cache lock.
    drop(retired);
    Ok(count)
}
impl Api {
    pub(crate) fn prune_cached_views(&self, clock_ms: u64) -> Result<Pruned> {
        // Each request retains its own Arc, so removing a cache owner cannot
        // release a snapshot still held by an in-flight request.
        let exact_in = reclaim(&self.cache, |value| {
            value.expires > clock_ms && value.view.is_valid()
        })?;
        let exact_out = reclaim(&self.exact_out_cache, |value| {
            value.expires > clock_ms && value.view.is_valid()
        })?;
        let preparations = reclaim(&self.preparations, |value| {
            value.context.deadline_ms > clock_ms && value.view.is_valid()
        })?;
        Ok(Pruned {
            exact_in,
            exact_out,
            preparations,
        })
    }
    pub(crate) async fn maintain_cached_views(&self) {
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            match self.prune_cached_views(now_ms()) {
                Ok(pruned) if pruned.total() != 0 => self.logger.event("info", "quote_cache_reclaimed", json!({
                    "exact_in":pruned.exact_in, "exact_out":pruned.exact_out, "preparations":pruned.preparations
                })),
                Err(error) => self.logger.event("error", "quote_cache_reclaim_failed", json!({"error_code":error.0})),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) async fn populated() -> (Api, Snapshot, String, String) {
        let data: Value =
            serde_json::from_str(include_str!("../../../test/contracts/v2-exact-out.json"))
                .unwrap();
        let mut snapshot: Snapshot =
            serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
        snapshot.pools = serde_json::from_value(data["grid"]["pools"].clone()).unwrap();
        let clock = snapshot.pools[0]
            .v2
            .as_ref()
            .unwrap()
            .swap
            .as_ref()
            .unwrap();
        snapshot.reference.block.number_or_slot = clock.bank_block_number;
        snapshot.reference.block.hash = clock.bank_block_hash.clone();
        snapshot.reference.block.timestamp_ms = clock.bank_unix_timestamp * 1000;
        snapshot.reference.scope_hash = xone_domain::scope_hash(&snapshot.pools).unwrap();
        let mut adapters: Vec<_> = snapshot
            .pools
            .iter()
            .map(|p| p.adapter_version.clone())
            .collect();
        adapters.sort();
        adapters.dedup();
        snapshot.reference.adapter_set_hash = hash(&adapters).unwrap();
        let mut config = xone_config::load(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
            "dex-router",
        )
        .unwrap();
        config.config.routing.max_state_age_ms = 315_360_000_000;
        config.config.api.quote_timeout_ms = 10000;
        config.config.routing.max_splits = 3;
        config.config.routing.split_parts = 4;
        config.config.routing.max_expansions = 10000;
        config.config.routing.max_candidates = 10;
        let api = Api::new(config).unwrap();
        api.replica.install_baseline(snapshot.clone()).unwrap();
        let request = json!({"chainKey":"evm:31337", "kind":"EXACT_OUT", "sellToken":data["grid"]["paths"][0][0],
            "buyToken":data["grid"]["paths"][0][1], "buyAmount":data["grid"]["target_output"],
            "maxSellAmount":data["grid"]["max_input"], "caller":data["context"]["sender"],
            "recipient":data["context"]["recipient"], "slippageBps":50, "verification":"FAST"});
        let exact_out = api
            .quote_exact_out(serde_json::from_value(request.clone()).unwrap())
            .await
            .unwrap();
        let mut request = request;
        request["kind"] = "EXACT_IN".into();
        request["sellAmount"] = exact_out["requiredSellAmount"].clone();
        request.as_object_mut().unwrap().remove("buyAmount");
        request.as_object_mut().unwrap().remove("maxSellAmount");
        let exact_in = api
            .quote(serde_json::from_value(request).unwrap())
            .await
            .unwrap();
        let cached = api.cache.lock().unwrap().values().next().unwrap().clone();
        solana_simulation::store(
            &api,
            cached.view,
            cached.plan.context,
            snapshot.pools[0].clone(),
            &json!({"ready":true,"preparationHash":"prepared","quoteId":exact_in["quoteId"]}),
            Some(xone_execution_preparation::AccountSnapshot {
                schema_version: 1,
                chain_key: "solana:fixture".into(),
                genesis_hash: "fixture".into(),
                observed_slot: 1,
                accounts: BTreeMap::new(),
            }),
        )
        .unwrap();
        (
            api,
            snapshot,
            exact_in["quoteId"].as_str().unwrap().into(),
            exact_out["quoteId"].as_str().unwrap().into(),
        )
    }

    #[test]
    fn reclaimed_values_are_destroyed_after_cache_lock_is_released() {
        struct Probe {
            cache: std::sync::Weak<Mutex<BTreeMap<String, Probe>>>,
            dropped: Arc<AtomicBool>,
        }
        impl Drop for Probe {
            fn drop(&mut self) {
                if let Some(cache) = self.cache.upgrade() {
                    assert!(
                        cache.try_lock().is_ok(),
                        "Reclaimed value destroyed under cache lock"
                    );
                    self.dropped.store(true, Ordering::Release);
                }
            }
        }
        let cache = Arc::new(Mutex::new(BTreeMap::new()));
        let dropped = Arc::new(AtomicBool::new(false));
        cache.lock().unwrap().insert(
            "expired".into(),
            Probe {
                cache: Arc::downgrade(&cache),
                dropped: dropped.clone(),
            },
        );
        assert_eq!(reclaim(&cache, |_| false).unwrap(), 1);
        assert!(dropped.load(Ordering::Acquire));
        assert!(cache.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn expiry_boundary_prunes_all_three_caches_and_retains_live_entries() {
        let (api, _, exact_in, exact_out) = populated().await;
        {
            let mut cache = api.cache.lock().unwrap();
            cache.get_mut(&exact_in).unwrap().expires = 101;
            let mut expired = cache[&exact_in].clone();
            expired.expires = 100;
            cache.insert("expired".into(), expired);
        }
        {
            let mut cache = api.exact_out_cache.lock().unwrap();
            cache.get_mut(&exact_out).unwrap().expires = 101;
            let mut expired = cache[&exact_out].clone();
            expired.expires = 100;
            cache.insert("expired".into(), expired);
        }
        {
            let mut cache = api.preparations.lock().unwrap();
            cache.get_mut("prepared").unwrap().context.deadline_ms = 101;
            let mut expired = cache["prepared"].clone();
            expired.context.deadline_ms = 100;
            cache.insert("expired".into(), expired);
        }
        assert_eq!(
            api.prune_cached_views(100).unwrap(),
            Pruned {
                exact_in: 1,
                exact_out: 1,
                preparations: 1
            }
        );
        assert!(api.cache.lock().unwrap().contains_key(&exact_in));
        assert!(api.exact_out_cache.lock().unwrap().contains_key(&exact_out));
        assert!(api.preparations.lock().unwrap().contains_key("prepared"));
        assert_eq!(api.prune_cached_views(100).unwrap(), Pruned::default());
        assert_eq!(
            api.prune_cached_views(101).unwrap(),
            Pruned {
                exact_in: 1,
                exact_out: 1,
                preparations: 1
            }
        );
    }

    #[tokio::test]
    async fn revocation_reclaims_cached_view_and_account_capture_without_more_requests() {
        let (api, mut snapshot, _, _) = populated().await;
        let view = api.replica.view("evm:31337").unwrap();
        let weak_view = Arc::downgrade(&view);
        let weak_capture = Arc::downgrade(
            api.preparations.lock().unwrap()["prepared"]
                .accounts
                .as_ref()
                .unwrap(),
        );
        snapshot.reference.online_generation += 1;
        snapshot.reference.journal_seq += 1;
        api.replica.install_baseline(snapshot).unwrap();
        assert!(!view.is_valid());
        drop(view);
        assert!(weak_view.upgrade().is_some());
        assert!(weak_capture.upgrade().is_some());
        assert_eq!(
            api.prune_cached_views(0).unwrap(),
            Pruned {
                exact_in: 1,
                exact_out: 1,
                preparations: 1
            }
        );
        assert!(weak_view.upgrade().is_none());
        assert!(weak_capture.upgrade().is_none());
        assert!(api.replica.view("evm:31337").unwrap().is_valid());
    }

    #[tokio::test]
    async fn in_flight_handle_survives_cache_reclamation_until_request_drops_it() {
        let (api, mut snapshot, exact_in, _) = populated().await;
        let in_flight = api.cache.lock().unwrap()[&exact_in].clone();
        let weak = Arc::downgrade(&in_flight.view);
        snapshot.reference.online_generation += 1;
        snapshot.reference.journal_seq += 1;
        api.replica.install_baseline(snapshot).unwrap();
        api.prune_cached_views(u64::MAX).unwrap();
        assert!(weak.upgrade().is_some());
        assert_eq!(in_flight.response["quoteId"], exact_in);
        assert!(!in_flight.view.is_valid());
        drop(in_flight);
        assert!(weak.upgrade().is_none());
    }
}

#[cfg(test)]
mod task_tests {
    use super::*;
    #[tokio::test]
    async fn maintenance_runs_without_quote_traffic_and_can_be_cancelled() {
        let (api, _, exact_in, exact_out) = super::tests::populated().await;
        api.cache
            .lock()
            .unwrap()
            .get_mut(&exact_in)
            .unwrap()
            .expires = 0;
        api.exact_out_cache
            .lock()
            .unwrap()
            .get_mut(&exact_out)
            .unwrap()
            .expires = 0;
        api.preparations
            .lock()
            .unwrap()
            .get_mut("prepared")
            .unwrap()
            .context
            .deadline_ms = 0;
        let maintenance = api.clone();
        let task = tokio::spawn(async move { maintenance.maintain_cached_views().await });
        let reclaimed = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let empty = api.cache.lock().unwrap().is_empty()
                    && api.exact_out_cache.lock().unwrap().is_empty()
                    && api.preparations.lock().unwrap().is_empty();
                if empty {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        reclaimed.expect("Idle maintenance did not reclaim expired cache entries");
        assert_eq!(api.prune_cached_views(u64::MAX).unwrap(), Pruned::default());
    }
    #[tokio::test]
    async fn dropping_server_task_guard_aborts_its_workers() {
        let (started, ready) = tokio::sync::oneshot::channel();
        let (released, done) = tokio::sync::oneshot::channel();
        struct Released(Option<tokio::sync::oneshot::Sender<()>>);
        impl Drop for Released {
            fn drop(&mut self) {
                let _ = self.0.take().unwrap().send(());
            }
        }
        let task = tokio::spawn(async move {
            let _released = Released(Some(released));
            let _ = started.send(());
            std::future::pending::<()>().await;
        });
        ready.await.unwrap();
        let workers = BackgroundTasks(vec![task]);
        drop(workers);
        tokio::time::timeout(Duration::from_secs(1), done)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn normal_http_service_reclaims_idle_caches_and_stops_maintenance_on_shutdown() {
        let (mut api, snapshot, exact_in, exact_out) = super::tests::populated().await;
        let source_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let source_url = format!("http://{}", source_listener.local_addr().unwrap());
        let source_router = Router::new().route(
            "/internal/chains/{chain}/baseline",
            get({
                let snapshot = snapshot.clone();
                move || {
                    let snapshot = snapshot.clone();
                    async move { Json(snapshot) }
                }
            }),
        );
        let source_task = tokio::spawn(async move {
            axum::serve(source_listener, source_router).await.unwrap();
        });
        let mut source_guard = BackgroundTasks(vec![source_task]);
        let config = Arc::make_mut(&mut api.config);
        config.config.journal.source_urls = vec![source_url];
        config
            .config
            .chains
            .retain(|chain| chain.key == "evm:31337");
        let mut retained = api.cache.lock().unwrap()[&exact_in].clone();
        retained.expires = 0;
        api.cache
            .lock()
            .unwrap()
            .get_mut(&exact_in)
            .unwrap()
            .expires = 0;
        api.exact_out_cache
            .lock()
            .unwrap()
            .get_mut(&exact_out)
            .unwrap()
            .expires = 0;
        api.preparations
            .lock()
            .unwrap()
            .get_mut("prepared")
            .unwrap()
            .context
            .deadline_ms = 0;
        let capture = Arc::downgrade(
            api.preparations.lock().unwrap()["prepared"]
                .accounts
                .as_ref()
                .unwrap(),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(serve(api.clone(), listener, async move {
            let _ = stopped.await;
        }));
        let health = reqwest::Client::new()
            .get(format!("{url}/health/live"))
            .send()
            .await
            .unwrap();
        assert_eq!(health.status(), reqwest::StatusCode::OK);
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let empty = api.cache.lock().unwrap().is_empty()
                    && api.exact_out_cache.lock().unwrap().is_empty()
                    && api.preparations.lock().unwrap().is_empty();
                if empty {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("Normal service did not reclaim idle caches");
        assert!(capture.upgrade().is_none());
        assert!(api.replica.view("evm:31337").unwrap().is_valid());
        stop.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        api.cache
            .lock()
            .unwrap()
            .insert("after-shutdown".into(), retained);
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert!(api.cache.lock().unwrap().contains_key("after-shutdown"));
        source_guard.stop().await;
    }
}
