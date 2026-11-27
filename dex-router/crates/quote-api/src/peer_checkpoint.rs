use super::{failure, Api, Duration, HttpError};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;
use xone_domain::{err, Result, SnapshotRef};
use xone_kafka_transport::{Bootstrap, Cursor};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PeerCheckpoint {
    reference: SnapshotRef,
    cursor: Cursor,
}

pub(super) async fn checkpoint(
    State(api): State<Api>,
    headers: HeaderMap,
    Path(chain): Path<String>,
) -> std::result::Result<Json<PeerCheckpoint>, HttpError> {
    if api.config.token.is_empty() || api.config.config.journal.transport != "kafka" {
        return Err(failure(err("CHECKPOINT_PEER_UNAVAILABLE")));
    }
    api.authorize(&headers).map_err(failure)?;
    let (reference, cursor) = api
        .applied_cursors
        .lock()
        .map_err(|_| failure(err("CURSOR_LOCK")))?
        .get(&chain)
        .cloned()
        .ok_or_else(|| failure(err("CHECKPOINT_PEER_UNAVAILABLE")))?;
    let view = api.replica.view(&chain).map_err(failure)?;
    api.check_view(&view).map_err(failure)?;
    if reference != view.reference {
        return Err(failure(err("CHECKPOINT_PEER_UNAVAILABLE")));
    }
    Ok(Json(PeerCheckpoint { reference, cursor }))
}

impl Api {
    pub(super) async fn restore_peer_checkpoint(&self, chain: &str) -> Result<()> {
        let mut candidates = JoinSet::new();
        for source in &self.config.config.journal.checkpoint_peer_urls {
            let path = format!("{source}/internal/chains/{chain}/peer-checkpoint");
            let api = self.clone();
            candidates.spawn(async move {
                let raw = api.fetch_raw_limited(&path, 65536).await?;
                serde_json::from_slice::<PeerCheckpoint>(&raw)
                    .map_err(|_| err("CHECKPOINT_PEER_RESPONSE_INVALID"))
            });
        }
        while let Some(candidate) = candidates.join_next().await {
            let result = async {
                let candidate = candidate.map_err(|_| err("CHECKPOINT_PEER_REQUEST_TASK"))??;
                let boot = self.attest_peer_checkpoint(chain, candidate).await?;
                self.restore_kafka_checkpoint(chain, boot, "PEER_CHECKPOINT_AND_KAFKA")
                    .await
            }
            .await;
            match result {
                Ok(()) => return Ok(()),
                Err(error) => self.logger.event(
                    "debug",
                    "peer_checkpoint_unavailable",
                    serde_json::json!({"chain_key":chain,"error_code":error.0}),
                ),
            }
        }
        Err(err("CHECKPOINT_PEER_UNAVAILABLE"))
    }

    async fn attest_peer_checkpoint(
        &self,
        chain: &str,
        candidate: PeerCheckpoint,
    ) -> Result<Bootstrap> {
        let cfg = self.config.clone();
        let journal = &cfg.config.journal;
        let owner = cfg
            .config
            .chains
            .iter()
            .find(|c| c.key == chain)
            .ok_or_else(|| err("UNKNOWN_CHAIN"))?;
        let block = &candidate.reference.block;
        let now = xone_observability::now_ms();
        if block.chain_key != chain
            || candidate.reference.owner_service != owner.owner_service
            || candidate.reference.owner_assignment_version != owner.assignment_version
            || journal.kafka.topics.get(chain) != Some(&candidate.cursor.topic)
            || candidate.cursor.partition != 0
            || candidate.cursor.offset < 1
            || block.timestamp_ms > now.saturating_add(30_000)
            || now.saturating_sub(block.timestamp_ms) > cfg.config.startup.max_baseline_age_ms
        {
            return Err(err("CHECKPOINT_PEER_IDENTITY"));
        }
        tokio::task::spawn_blocking(move || {
            let journal = &cfg.config.journal;
            let unit = xone_kafka_transport::checkpoint_at(
                &journal.kafka.brokers,
                &candidate.cursor,
                journal.max_payload_bytes,
                journal.kafka.max_tail_records,
                Duration::from_millis(journal.request_timeout_ms),
            )?;
            if unit.snapshot.reference != candidate.reference
                || !matches!(
                    unit.kind.as_str(),
                    "ONLINE_BASELINE_COMMIT" | "BLOCK_COMMIT" | "REORG_COMMIT"
                )
            {
                return Err(err("CHECKPOINT_PEER_UNATTESTED"));
            }
            Ok(Bootstrap {
                snapshot: unit.snapshot,
                cursor: candidate.cursor,
            })
        })
        .await
        .map_err(|_| err("KAFKA_READER_TASK"))?
    }
}
