use super::{failure, Duration, HeaderMap, Json, Ordering, Parser, State, StatusCode, Value};
use std::sync::{atomic::AtomicBool, Arc};
use xone_domain::{err, Result, SnapshotRef};
use xone_kafka_transport::checkpoint::local;
#[path = "service_checkpoint_remote.rs"]
mod remote;
use remote::{Remote, RemotePublication};
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
    async fn checkpoint_guard(&self, key: &str, reference: &SnapshotRef) -> Result<()> {
        if *self.stream_stop.borrow() {
            return Err(err("CHECKPOINT_OWNER_CLOSED"));
        }
        let after = self.checked_current(key).await?;
        if *self.stream_stop.borrow() || !same_era(reference, &after.reference) {
            return Err(err("CHECKPOINT_SOURCE_REVOKED"));
        }
        Ok(())
    }
    async fn current_checkpoint_catalog(&self, key: &str) -> Result<RemotePublication> {
        if !self.started.load(Ordering::Acquire) || *self.stream_stop.borrow() {
            return Err(err("CHECKPOINT_OWNER_CLOSED"));
        }
        let before = self.checked_current(key).await?;
        let c = self
            .config
            .config
            .journal
            .checkpoint_store
            .as_ref()
            .ok_or_else(|| err("CHECKPOINT_STORE_REQUIRED"))?;
        let boot = {
            let store = self
                .stores
                .get(key)
                .ok_or_else(|| err("UNKNOWN_CHAIN"))?
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?;
            let boot = store.bootstrap()?;
            if boot.snapshot.reference != before.reference {
                return Err(err("SCOPE_READY_REFERENCE_CHANGED"));
            }
            boot
        };
        let copy = self.clone();
        let maximum = self.config.config.journal.max_payload_bytes;
        let key = key.to_owned();
        let reference = before.reference.clone();
        let c = c.clone();
        let result = tokio::task::spawn_blocking(move || {
            let (manifest, raw, _) = local::encode(&boot, maximum)?;
            use sha2::{Digest, Sha256};
            let pin = hex::encode(Sha256::digest(&raw));
            let path = copy
                .config
                .resolve(&copy.config.config.state.path)
                .join("checkpoints")
                .join(key.replace(':', "-"))
                .join("public-v2")
                .join(&manifest.sha256)
                .join("manifest.json");
            let loaded = local::read(&path, &pin, maximum)?;
            if loaded.snapshot.reference != reference || loaded.cursor != boot.cursor {
                return Err(err("CHECKPOINT_CATALOG_IDENTITY"));
            }
            let remote = Remote::new(
                &c,
                "",
                Duration::from_millis(copy.config.config.journal.request_timeout_ms),
            )?;
            let expected = RemotePublication {
                manifest_url: remote.manifest_url(&manifest),
                manifest_sha256: pin,
                manifest,
            };
            let encoded = serde_json::to_vec(&expected)
                .map_err(|_| err("CHECKPOINT_CERTIFICATE_ENCODING"))?;
            if local::certificate(&path, &remote.marker_name())? != encoded {
                return Err(err("CHECKPOINT_CATALOG_UNPUBLISHED"));
            }
            Ok::<_, xone_domain::Error>(expected)
        })
        .await
        .map_err(|_| err("CHECKPOINT_FILE_TASK"))??;
        let after = self
            .checked_current(&before.reference.block.chain_key)
            .await?;
        if *self.stream_stop.borrow() || after.reference != before.reference {
            return Err(err("CHECKPOINT_CATALOG_SOURCE_CHANGED"));
        }
        let cursor = self.stores[&before.reference.block.chain_key]
            .lock()
            .map_err(|_| err("STATE_LOCK_POISONED"))?
            .bootstrap()?
            .cursor;
        if cursor.topic != result.manifest.cursor.topic
            || cursor.partition != result.manifest.cursor.partition
            || cursor.offset.to_string() != result.manifest.cursor.next_offset
        {
            return Err(err("CHECKPOINT_CATALOG_SOURCE_CHANGED"));
        }
        Ok(result)
    }

    async fn write_checkpoints(&self) -> Result<Value> {
        let mut gate = self.checkpoint_gate.clone().lock_owned().await;
        if !self.started.load(Ordering::Acquire) || *self.stream_stop.borrow() {
            return Err(err("CHECKPOINT_OWNER_CLOSED"));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let _cancel_on_drop = CancelOnDrop(cancelled.clone());
        let mut publications = Vec::new();
        let mut remote_publications = Vec::new();
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
            if let Some(c) = &self.config.config.journal.checkpoint_store {
                let token = self
                    .config
                    .config
                    .journal
                    .source_auth_tokens
                    .get(&c.base_url)
                    .map(String::as_str)
                    .unwrap_or("");
                let remote = Remote::new(
                    c,
                    token,
                    Duration::from_millis(self.config.config.journal.request_timeout_ms),
                )?;
                self.checkpoint_guard(&chain.key, &before.reference).await?;
                let path = publication.manifest_path.clone();
                let pin = publication.manifest_sha256.clone();
                let (objects, returned_gate) = tokio::task::spawn_blocking(move || {
                    let objects = local::objects(&path, &pin, maximum);
                    (objects, gate)
                })
                .await
                .map_err(|_| err("CHECKPOINT_FILE_TASK"))?;
                gate = returned_gate;
                let (manifest, raw, payload) = objects?;
                remote
                    .payload(&raw, &payload, &publication.manifest_sha256, maximum)
                    .await?;
                self.checkpoint_guard(&chain.key, &before.reference).await?;
                let published = remote
                    .manifest(&manifest, &raw, &publication.manifest_sha256, maximum)
                    .await?;
                self.checkpoint_guard(&chain.key, &before.reference).await?;
                let certificate = serde_json::to_vec(&published)
                    .map_err(|_| err("CHECKPOINT_CERTIFICATE_ENCODING"))?;
                let tag = remote.marker_name();
                let p = publication.clone();
                let key = chain.key.clone();
                let reference = before.reference.clone();
                let copy = self.clone();
                let stopped = cancelled.clone();
                let (saved, returned_gate) = tokio::task::spawn_blocking(move || {
                    let saved = local::certify(&p, maximum, &tag, &certificate, || {
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
                    (saved, gate)
                })
                .await
                .map_err(|_| err("CHECKPOINT_FILE_TASK"))?;
                gate = returned_gate;
                saved?;
                self.checkpoint_guard(&chain.key, &before.reference).await?;
                remote_publications.push(published);
            }
            publications.push(publication);
        }
        Ok(
            serde_json::json!({"manifests":[],"public_checkpoints":publications,"remote_checkpoints":remote_publications}),
        )
    }
}
pub(super) async fn publish(
    State(parser): State<Parser>,
    headers: HeaderMap,
) -> std::result::Result<Json<Value>, (StatusCode, Json<Value>)> {
    parser.authorize(&headers).map_err(failure)?;
    let mut stop = parser.stream_stop.subscribe();
    tokio::select! {
        result=tokio::time::timeout(Duration::from_millis(parser.config.config.journal.request_timeout_ms),parser.write_checkpoints())=>result.map_err(|_|failure(err("CHECKPOINT_DEADLINE")))?.map(Json).map_err(failure),
        _=stop.changed()=>Err(failure(err("CHECKPOINT_CANCELLED"))),
    }
}
#[cfg(test)]
#[path = "service_checkpoint_tests.rs"]
mod tests;

pub(super) async fn catalog(
    State(parser): State<Parser>,
    headers: HeaderMap,
    super::Path(chain): super::Path<String>,
) -> std::result::Result<Json<RemotePublication>, (StatusCode, Json<Value>)> {
    parser.authorize(&headers).map_err(failure)?;
    tokio::time::timeout(
        Duration::from_millis(parser.config.config.journal.request_timeout_ms),
        parser.current_checkpoint_catalog(&chain),
    )
    .await
    .map_err(|_| failure(err("CHECKPOINT_DEADLINE")))?
    .map(Json)
    .map_err(failure)
}
#[cfg(test)]
#[path = "service_checkpoint_remote_tests.rs"]
mod remote_tests;
