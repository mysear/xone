use super::*;

impl Parser {
    // Configuration, RocksDB/WAL opening and shared PG initialization have
    // already succeeded. Per-chain acquisition failures must not gate peers.
    pub(super) async fn initialize_chain(&self, chain: &Chain, startup: bool) -> Result<()> {
        if self.source.is_none() {
            if let Some(stream) = self.config.config.yellowstone.get(&chain.key) {
                self.recover_revocation(chain).await?;
                // Register under the same gate that close drains. If shutdown
                // won the gate, no task may be installed after its drain.
                let mut tasks = self.stream_tasks.lock().await;
                self.check_startup_open()?;
                let task = tokio::spawn(self.clone().run_stream(
                    chain.clone(),
                    stream.clone(),
                    self.stream_stop.subscribe(),
                ));
                tasks.push(task);
                return Ok(());
            }
        }
        self.reanchor(chain, startup).await
    }

    pub(super) fn check_startup_open(&self) -> Result<()> {
        if *self.stream_stop.borrow() {
            self.started.store(false, Ordering::Release);
            // A read/commit already in progress settles under the actor before
            // shutdown drains it. Do not keep its scope visible after close.
            for store in self.stores.values() {
                store.lock().map(|mut store| store.suspend()).ok();
            }
            return Err(err("PARSER_ALREADY_STARTED_OR_CLOSED"));
        }
        Ok(())
    }

    pub(super) fn pending_start(&self, chain: &str) -> bool {
        self.pending_starts
            .lock()
            .map(|pending| pending.contains_key(chain))
            .unwrap_or(true)
    }

    pub(super) fn clear_pending_start(&self, chain: &str) -> Result<()> {
        self.pending_starts
            .lock()
            .map_err(|_| err("STATE_LOCK_POISONED"))?
            .remove(chain);
        Ok(())
    }

    pub(super) async fn isolate_start_failure(
        &self,
        chain: &Chain,
        error: &xone_domain::Error,
    ) -> Result<()> {
        // revoke_scope suspends locally before any durable revocation attempt.
        // An unavailable journal must never leave a failed scope readable.
        if let Err(failure) = self.revoke_scope(chain).await {
            self.logger.event(
                "error",
                "scope_revocation_failed",
                json!({"chain_key":chain.key,"error_code":failure.0}),
            );
        }
        self.stores[&chain.key]
            .lock()
            .map(|mut store| store.suspend())
            .ok();
        if let Some(pg) = self.control.get() {
            let mut writer = self.writers[&chain.key].lock().await;
            if let Some(active) = writer.as_mut() {
                active.close(pg, &self.config).await;
            }
            *writer = None;
        }
        self.pending_starts
            .lock()
            .map_err(|_| err("STATE_LOCK_POISONED"))?
            .insert(chain.key.clone(), error.0.clone());
        self.logger.event(
            "error",
            "chain_start_failed",
            json!({"chain_key":chain.key,"error_code":error.0}),
        );
        Ok(())
    }

    // Own the newly acquired writer until every local reconciliation gate has
    // succeeded. Its caller has not installed it in writers yet, so startup,
    // Tick and scope recovery cannot otherwise release it after a store error.
    pub(super) async fn reconcile_opened_writer(
        &self,
        pg: &Postgres,
        chain: &Chain,
        mut opened: Writer,
        recovery: Option<xone_kafka_transport::Cursor>,
    ) -> Result<Writer> {
        let reconciled = (|| {
            let mut store = self.stores[&chain.key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?;
            let result = if let Some(cursor) = recovery {
                store.apply_prepared(cursor)
            } else {
                store.discard_prepared()
            };
            store.suspend();
            if result.is_ok() {
                store.set_authority_deadline(opened.deadline(&self.config));
            }
            result
        })();
        if let Err(error) = reconciled {
            // close uses its own bounded PG allowance; neither a poisoned
            // mutex nor a failed RocksDB write can skip the lease release.
            opened.close(pg, &self.config).await;
            return Err(error);
        }
        Ok(opened)
    }
}

#[cfg(test)]
#[path = "service_startup_tests.rs"]
mod tests;
