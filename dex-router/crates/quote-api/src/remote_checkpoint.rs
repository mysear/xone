//! Read a current owner-qualified catalogue, download only configured immutable
//! objects, and re-attest the same catalogue before handing state to recovery.
use super::{Api, Duration};
use xone_domain::{err, Result};
use xone_kafka_transport::{
    checkpoint::{
        self, recovery_compatible, recovery_target, RecoveryCatalog, RemotePublication as Catalog,
    },
    Bootstrap,
};

async fn download(
    client: &reqwest::Client,
    url: &str,
    token: Option<&str>,
    limit: usize,
) -> Result<Vec<u8>> {
    let request = client.get(url);
    let request = if let Some(token) = token.filter(|t| !t.is_empty()) {
        request.bearer_auth(token)
    } else {
        request
    };
    let mut response = request
        .send()
        .await
        .map_err(|_| err("CHECKPOINT_DOWNLOAD_NETWORK"))?;
    if response.status() != reqwest::StatusCode::OK
        || response.headers().contains_key("content-encoding")
        || response.content_length().is_some_and(|n| n > limit as u64)
    {
        return Err(err("CHECKPOINT_DOWNLOAD_STATUS_OR_LIMIT"));
    }
    let mut result = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| err("CHECKPOINT_DOWNLOAD_NETWORK"))?
    {
        if result.len().saturating_add(chunk.len()) > limit {
            return Err(err("CHECKPOINT_DOWNLOAD_LIMIT"));
        }
        result.extend_from_slice(&chunk);
    }
    Ok(result)
}
impl Api {
    async fn checkpoint_catalog(&self, source: &str, chain: &str) -> Result<Catalog> {
        let raw = self
            .fetch_raw_limited(
                &format!("{source}/internal/chains/{chain}/checkpoint-catalog"),
                65536,
            )
            .await?;
        serde_json::from_slice(&raw).map_err(|_| err("CHECKPOINT_CATALOG_INVALID"))
    }
    async fn download_checkpoint(
        &self,
        catalog: &Catalog,
        chain: &str,
        timeout: Duration,
    ) -> Result<Bootstrap> {
        let store = self
            .config
            .config
            .journal
            .checkpoint_store
            .as_ref()
            .ok_or_else(|| err("CHECKPOINT_STORE_REQUIRED"))?;
        store.validate()?;
        let raw =
            serde_json::to_vec(&catalog.manifest).map_err(|_| err("CHECKPOINT_CATALOG_INVALID"))?;
        let maximum = self.config.config.journal.max_payload_bytes;
        let m = checkpoint::parse_manifest(&raw, &catalog.manifest_sha256, maximum)?;
        let configured = self
            .config
            .config
            .chains
            .iter()
            .find(|c| c.key == chain)
            .ok_or_else(|| err("UNKNOWN_CHAIN"))?;
        if m.reference.block.chain_key != chain
            || m.reference.owner_service != configured.owner_service
            || m.reference.owner_assignment_version != configured.assignment_version
            || self.config.config.journal.kafka.topics.get(chain) != Some(&m.cursor.topic)
        {
            return Err(err("CHECKPOINT_CATALOG_AUTHORITY"));
        }
        let directory = format!("{}/public-v2/{}", store.base_url, m.sha256);
        let manifest_url = format!("{directory}/manifest.json");
        if catalog.manifest_url != manifest_url {
            return Err(err("CHECKPOINT_CATALOG_LOCATION"));
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(timeout)
            .build()
            .map_err(|_| err("CHECKPOINT_DOWNLOAD_CLIENT"))?;
        let token = self
            .config
            .config
            .journal
            .source_auth_tokens
            .get(&store.base_url)
            .map(String::as_str);
        let manifest = download(&client, &manifest_url, token, 65536).await?;
        let downloaded = checkpoint::parse_manifest(&manifest, &catalog.manifest_sha256, maximum)?;
        if downloaded.sha256 != m.sha256 || manifest != raw {
            return Err(err("CHECKPOINT_CATALOG_CONTENT"));
        }
        let payload = download(
            &client,
            &format!("{directory}/bootstrap.pb"),
            token,
            downloaded.size as usize,
        )
        .await?;
        let boot = checkpoint::decode(&manifest, &payload, &catalog.manifest_sha256, maximum)?;
        Ok(boot)
    }
    pub(crate) async fn remote_bootstrap(&self, source: &str, chain: &str) -> Result<Bootstrap> {
        let timeout = Duration::from_millis(self.config.config.journal.request_timeout_ms);
        tokio::time::timeout(timeout, async {
            let store = self
                .config
                .config
                .journal
                .checkpoint_store
                .as_ref()
                .ok_or_else(|| err("CHECKPOINT_STORE_REQUIRED"))?;
            store.validate()?;
            let catalog = self.checkpoint_catalog(source, chain).await?;
            let boot = self.download_checkpoint(&catalog, chain, timeout).await?;
            // The catalogue attests exactly current state, not the newest-looking old file.
            let after = self.checkpoint_catalog(source, chain).await?;
            if serde_json::to_vec(&after).map_err(|_| err("CHECKPOINT_CATALOG_INVALID"))?
                != serde_json::to_vec(&catalog).map_err(|_| err("CHECKPOINT_CATALOG_INVALID"))?
            {
                return Err(err("CHECKPOINT_CATALOG_REVOKED"));
            }
            Ok(boot)
        })
        .await
        .map_err(|_| err("CHECKPOINT_DOWNLOAD_DEADLINE"))?
    }
    pub(crate) async fn recovery_bootstrap(
        &self,
        source: &str,
        chain: &str,
    ) -> Result<(Bootstrap, &'static str)> {
        if self.config.config.journal.checkpoint_store.is_some() {
            match self.remote_bootstrap(source, chain).await {
                Ok(boot) => return Ok((boot, "REMOTE_CHECKPOINT_AND_KAFKA")),
                Err(error) => self.logger.event(
                    "debug",
                    "remote_checkpoint_unavailable",
                    serde_json::json!({"chain_key":chain,"error_code":error.0}),
                ),
            }
        }
        let raw = self
            .fetch_raw(&format!("{source}/internal/chains/{chain}/bootstrap"))
            .await?;
        Ok((
            xone_kafka_transport::decode_bootstrap(&raw)?,
            "OWNER_CHECKPOINT_AND_KAFKA",
        ))
    }
}

#[cfg(test)]
mod replay_tests;
#[cfg(test)]
mod tests;

impl Api {
    async fn recovery_catalog(&self, source: &str, chain: &str) -> Result<RecoveryCatalog> {
        let raw = self
            .fetch_raw_limited(
                &format!("{source}/internal/chains/{chain}/checkpoint-recovery-catalog"),
                65536,
            )
            .await?;
        serde_json::from_slice(&raw).map_err(|_| err("CHECKPOINT_RECOVERY_CATALOG_INVALID"))
    }
    pub(crate) async fn replay_remote_checkpoint(
        &self,
        source: &str,
        chain: &str,
    ) -> Result<(Bootstrap, xone_kafka_transport::Reader)> {
        let timeout = Duration::from_millis(self.config.config.journal.request_timeout_ms);
        let deadline = std::time::Instant::now() + timeout;
        tokio::time::timeout(timeout, async {
            let catalog = self.recovery_catalog(source, chain).await?;
            let cfg = self.config.clone();
            let target = recovery_target(
                &catalog,
                cfg.config.journal.max_payload_bytes,
                cfg.config.journal.kafka.max_tail_records,
            )?;
            let boot = self
                .download_checkpoint(&catalog.checkpoint, chain, timeout)
                .await?;
            let expected = catalog.head_reference.clone();
            let version = catalog.schema_version;
            if expected.block.timestamp_ms > xone_observability::now_ms().saturating_add(30000)
                || xone_observability::now_ms().saturating_sub(expected.block.timestamp_ms)
                    > cfg.config.startup.max_baseline_age_ms
            {
                return Err(err("BASELINE_TOO_OLD"));
            }
            let chain_key = chain.to_owned();
            let (boot, reader) = tokio::task::spawn_blocking(move || {
                let remaining = || {
                    deadline
                        .checked_duration_since(std::time::Instant::now())
                        .filter(|d| !d.is_zero())
                        .ok_or_else(|| err("CHECKPOINT_REPLAY_DEADLINE"))
                };
                let k = &cfg.config.journal.kafka;
                let mut reader = xone_kafka_transport::Reader::open(
                    &k.brokers,
                    &format!("quote.{}.{}", chain_key, k.replica_id),
                    &boot.cursor,
                    cfg.config.journal.max_payload_bytes,
                    k.max_tail_records,
                    remaining()?,
                )?;
                reader.last_sequence = boot.snapshot.reference.journal_seq;
                // All intermediate views live only in this private replica.
                let staged = xone_state_replica::Replica::new(
                    cfg.config
                        .chains
                        .iter()
                        .map(|c| (c.key.clone(), c.owner_service.clone(), c.assignment_version))
                        .collect(),
                );
                staged.restore_owner_checkpoint(boot.snapshot.clone())?;
                let mut final_boot = boot;
                let mut frames = 0usize;
                while reader.cursor().offset < target.offset {
                    let before = reader.cursor();
                    let unit = reader.poll(remaining()?.min(Duration::from_millis(50)))?;
                    let position = reader.cursor();
                    if position != before {
                        frames = frames
                            .checked_add(1)
                            .ok_or_else(|| err("CHECKPOINT_REPLAY_FRAME_LIMIT"))?;
                    }
                    if frames > k.max_tail_records || position.offset > target.offset {
                        return Err(err("CHECKPOINT_REPLAY_CURSOR"));
                    }
                    if let Some(unit) = unit {
                        if !(unit.kind == "BLOCK_COMMIT"
                            || version == 2 && unit.kind == "ONLINE_BASELINE_COMMIT")
                            || !recovery_compatible(&unit.snapshot.reference, &expected, version)
                            || unit.snapshot.reference.journal_seq > expected.journal_seq
                        {
                            return Err(err("CHECKPOINT_REPLAY_REANCHOR"));
                        }
                        if unit.kind == "ONLINE_BASELINE_COMMIT" {
                            staged.install_baseline(unit.snapshot.clone())?;
                        } else {
                            staged.apply(xone_domain::Record {
                                reference: unit.snapshot.reference.clone(),
                                kind: unit.kind,
                                normalized_facts: unit.normalized_facts,
                                record_id: String::new(),
                                payload_hash: xone_domain::hash(&unit.snapshot.pools)?,
                                pools: unit.snapshot.pools.clone(),
                            })?;
                        }
                        final_boot = Bootstrap {
                            snapshot: unit.snapshot,
                            cursor: position,
                        };
                    }
                }
                if !reader.at_committed_boundary()
                    || reader.cursor() != target
                    || final_boot.cursor != target
                    || final_boot.snapshot.reference != expected
                {
                    return Err(err("CHECKPOINT_REPLAY_INCOMPLETE"));
                }
                staged.view(&chain_key)?;
                Ok::<_, xone_domain::Error>((final_boot, reader))
            })
            .await
            .map_err(|_| err("CHECKPOINT_REPLAY_TASK"))??;
            let after = self.recovery_catalog(source, chain).await?;
            if serde_json::to_vec(&after).map_err(|_| err("CHECKPOINT_CATALOG_INVALID"))?
                != serde_json::to_vec(&catalog).map_err(|_| err("CHECKPOINT_CATALOG_INVALID"))?
            {
                return Err(err("CHECKPOINT_CATALOG_REVOKED"));
            }
            Ok((boot, reader))
        })
        .await
        .map_err(|_| err("CHECKPOINT_REPLAY_DEADLINE"))?
    }
}

#[cfg(test)]
#[path = "remote_checkpoint/live_rust_owner_tests.rs"]
mod live_rust_owner_tests;
