use sha2::{Digest, Sha256};
use std::time::Duration;
use xone_domain::{err, Result};
pub(super) use xone_kafka_transport::checkpoint::RemotePublication;
use xone_kafka_transport::checkpoint::{self, Manifest};
pub struct Remote {
    base: String,
    token: String,
    client: reqwest::Client,
}
impl Remote {
    pub fn new(
        store: &xone_config::CheckpointStore,
        token: &str,
        timeout: Duration,
    ) -> Result<Self> {
        store.validate()?;
        if timeout.is_zero()
            || timeout > Duration::from_secs(60)
            || token.len() > 8192
            || !token.bytes().all(|b| (0x21..=0x7e).contains(&b))
        {
            return Err(err("CHECKPOINT_REMOTE_CONFIG"));
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(timeout)
            .build()
            .map_err(|_| err("CHECKPOINT_REMOTE_CLIENT"))?;
        Ok(Self {
            base: store.base_url.clone(),
            token: token.into(),
            client,
        })
    }
    fn request(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        let request = self.client.request(method, url);
        if self.token.is_empty() {
            request
        } else {
            request.bearer_auth(&self.token)
        }
    }
    pub fn manifest_url(&self, m: &Manifest) -> String {
        format!("{}/public-v2/{}/manifest.json", self.base, m.sha256)
    }
    pub fn candidate_name(&self) -> String {
        format!(
            "candidate-{}.json",
            hex::encode(Sha256::digest(self.base.as_bytes()))
        )
    }
    pub fn marker_name(&self) -> String {
        format!(
            "remote-{}.json",
            hex::encode(Sha256::digest(self.base.as_bytes()))
        )
    }
    async fn get(&self, url: &str, maximum: usize) -> Result<Option<Vec<u8>>> {
        let mut response = self
            .request(reqwest::Method::GET, url)
            .send()
            .await
            .map_err(|_| err("CHECKPOINT_REMOTE_NETWORK"))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if response.status() != reqwest::StatusCode::OK
            || response.headers().contains_key("content-encoding")
            || response
                .content_length()
                .is_some_and(|n| n > maximum as u64)
        {
            return Err(err("CHECKPOINT_REMOTE_STATUS_OR_LIMIT"));
        }
        let mut raw = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| err("CHECKPOINT_REMOTE_NETWORK"))?
        {
            if raw.len().saturating_add(chunk.len()) > maximum {
                return Err(err("CHECKPOINT_REMOTE_READ_LIMIT"));
            }
            raw.extend_from_slice(&chunk);
        }
        Ok(Some(raw))
    }
    async fn put(&self, url: &str, raw: &[u8]) -> Result<()> {
        if let Some(existing) = self.get(url, raw.len()).await? {
            return if existing == raw {
                Ok(())
            } else {
                Err(err("CHECKPOINT_REMOTE_CONFLICT"))
            };
        }
        let response = self
            .request(reqwest::Method::PUT, url)
            .header("If-None-Match", "*")
            .header("content-type", "application/octet-stream")
            .body(raw.to_vec())
            .send()
            .await
            .map_err(|_| err("CHECKPOINT_REMOTE_NETWORK"))?;
        let status = response.status();
        drop(response);
        if ![
            reqwest::StatusCode::OK,
            reqwest::StatusCode::CREATED,
            reqwest::StatusCode::NO_CONTENT,
            reqwest::StatusCode::PRECONDITION_FAILED,
            reqwest::StatusCode::CONFLICT,
        ]
        .contains(&status)
        {
            return Err(err("CHECKPOINT_REMOTE_PUT_STATUS"));
        }
        if self.get(url, raw.len()).await?.as_deref() != Some(raw) {
            return Err(err("CHECKPOINT_REMOTE_READBACK"));
        }
        Ok(())
    }
    pub async fn payload(
        &self,
        manifest: &[u8],
        payload: &[u8],
        pin: &str,
        max: usize,
    ) -> Result<Manifest> {
        let m = checkpoint::parse_manifest(manifest, pin, max)?;
        checkpoint::decode(manifest, payload, pin, max)?;
        self.put(
            &format!("{}/public-v2/{}/bootstrap.pb", self.base, m.sha256),
            payload,
        )
        .await?;
        Ok(m)
    }
    pub async fn manifest(
        &self,
        m: &Manifest,
        raw: &[u8],
        pin: &str,
        max: usize,
    ) -> Result<RemotePublication> {
        let checked = checkpoint::parse_manifest(raw, pin, max)?;
        if checked.sha256 != m.sha256 {
            return Err(err("CHECKPOINT_REMOTE_OBJECT_ID"));
        }
        let url = self.manifest_url(m);
        self.put(&url, raw).await?;
        Ok(RemotePublication {
            manifest_url: url,
            manifest_sha256: pin.into(),
            manifest: m.clone(),
        })
    }
}
