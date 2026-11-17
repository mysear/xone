//! Read a current owner-qualified catalogue, download only configured immutable
//! objects, and re-attest the same catalogue before handing state to recovery.
use super::{Api, Duration};
use serde::{Deserialize, Serialize};
use xone_domain::{err, Result};
use xone_kafka_transport::{checkpoint, Bootstrap};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    manifest_url: String,
    manifest_sha256: String,
    manifest: checkpoint::Manifest,
}
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
            let raw = serde_json::to_vec(&catalog.manifest)
                .map_err(|_| err("CHECKPOINT_CATALOG_INVALID"))?;
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
            let downloaded =
                checkpoint::parse_manifest(&manifest, &catalog.manifest_sha256, maximum)?;
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
mod tests;
