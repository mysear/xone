use super::{failure, Duration, HeaderMap, Json, Ordering, Parser, State, StatusCode, Value};
use std::sync::{atomic::AtomicBool, Arc};
use xone_domain::{err, Result, SnapshotRef};
use xone_kafka_transport::checkpoint::local;
fn same_era(a: &SnapshotRef, b: &SnapshotRef) -> bool {
    a.block.chain_key == b.block.chain_key
        && a.owner_service == b.owner_service
        && a.owner_assignment_version == b.owner_assignment_version
        && a.writer_epoch == b.writer_epoch
        && a.canonical_epoch == b.canonical_epoch
        && a.online_generation == b.online_generation
        && a.schema_version == b.schema_version
        && a.scope_hash == b.scope_hash
        && a.adapter_set_hash == b.adapter_set_hash
}
struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
impl Parser {
    async fn write_checkpoints(&self) -> Result<Value> {
        let mut gate = self.checkpoint_gate.clone().lock_owned().await;
        if !self.started.load(Ordering::Acquire) || *self.stream_stop.borrow() {
            return Err(err("CHECKPOINT_OWNER_CLOSED"));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let _cancel_on_drop = CancelOnDrop(cancelled.clone());
        let mut publications = Vec::new();
        for chain in &self.config.config.chains {
            let before = self.checked_current(&chain.key).await?;
            let boot = {
                let store = self.stores[&chain.key]
                    .lock()
                    .map_err(|_| err("STATE_LOCK_POISONED"))?;
                let boot = match store.bootstrap() {
                    Ok(boot) => boot,
                    Err(error)
                        if self.config.config.journal.transport == "http-development"
                            && error.0 == "BOOTSTRAP_NOT_READY" =>
                    {
                        continue
                    }
                    Err(error) => return Err(error),
                };
                if boot.snapshot.reference != before.reference {
                    return Err(err("SCOPE_READY_REFERENCE_CHANGED"));
                }
                boot
            };
            let directory = self
                .config
                .resolve(&self.config.config.state.path)
                .join("checkpoints")
                .join(chain.key.replace(':', "-"));
            let reference = boot.snapshot.reference.clone();
            let copy = self.clone();
            let key = chain.key.clone();
            let stopped = cancelled.clone();
            let maximum = self.config.config.journal.max_payload_bytes;
            let (publication, returned_gate) = tokio::task::spawn_blocking(move || {
                let result = local::write(&directory, &boot, maximum, || {
                    if stopped.load(Ordering::Acquire) || *copy.stream_stop.borrow() {
                        return Err(err("CHECKPOINT_CANCELLED"));
                    }
                    let current = copy.stores[&key]
                        .lock()
                        .map_err(|_| err("STATE_LOCK_POISONED"))?
                        .current()
                        .ok_or_else(|| err("CHECKPOINT_SOURCE_REVOKED"))?;
                    if !same_era(&reference, &current.reference) {
                        return Err(err("CHECKPOINT_SOURCE_REVOKED"));
                    }
                    Ok(())
                });
                (result, gate)
            })
            .await
            .map_err(|_| err("CHECKPOINT_FILE_TASK"))?;
            gate = returned_gate;
            let publication = publication?;
            let after = self.checked_current(&chain.key).await?;
            if *self.stream_stop.borrow() || !same_era(&before.reference, &after.reference) {
                return Err(err("CHECKPOINT_SOURCE_REVOKED"));
            }
            publications.push(publication);
        }
        Ok(
            serde_json::json!({"manifests":[],"public_checkpoints":publications,"remote_checkpoints":[]}),
        )
    }
}
pub(super) async fn publish(
    State(parser): State<Parser>,
    headers: HeaderMap,
) -> std::result::Result<Json<Value>, (StatusCode, Json<Value>)> {
    parser.authorize(&headers).map_err(failure)?;
    tokio::time::timeout(
        Duration::from_millis(parser.config.config.journal.request_timeout_ms),
        parser.write_checkpoints(),
    )
    .await
    .map_err(|_| failure(err("CHECKPOINT_DEADLINE")))?
    .map(Json)
    .map_err(failure)
}
#[cfg(test)]
#[path = "service_checkpoint_tests.rs"]
mod tests;
