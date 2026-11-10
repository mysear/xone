//! Normal public history RPC worker. Network I/O never holds the online actor.
use crate::{history_queue::*, rpc::Rpc, store::Store};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::sync::watch;
use xone_config::ConfigSnapshot;
use xone_domain::{err, Result};
fn value(r: &RpcReceipt) -> Result<Value> {
    let v: Value = serde_json::from_slice(&r.response).map_err(|_| err("HISTORY_RPC_JSON"))?;
    if v["jsonrpc"] != "2.0" || v["id"] != 1 || !v["error"].is_null() || v["result"].is_null() {
        return Err(err("HISTORY_RPC_UNKNOWN"));
    }
    Ok(v["result"].clone())
}
fn hash32(s: &str) -> bool {
    bs58::decode(s).into_vec().is_ok_and(|b| b.len() == 32)
}
fn header(slot: u64, v: &Value, now: u64) -> Result<(String, u64, String)> {
    let h = v["blockhash"]
        .as_str()
        .ok_or_else(|| err("HISTORY_BLOCK_HEADER"))?;
    let p = v["previousBlockhash"]
        .as_str()
        .ok_or_else(|| err("HISTORY_BLOCK_HEADER"))?;
    let parent = v["parentSlot"]
        .as_u64()
        .ok_or_else(|| err("HISTORY_BLOCK_HEADER"))?;
    if !hash32(h)
        || !hash32(p)
        || (slot != 0 && parent >= slot)
        || (slot == 0 && parent != 0)
        || v["blockTime"]
            .as_u64()
            .is_some_and(|t| t.saturating_mul(1000) > now.saturating_add(30000))
    {
        return Err(err("HISTORY_BLOCK_HEADER_OR_FUTURE"));
    }
    Ok((h.into(), parent, p.into()))
}
/// Revalidate retained raw receipts, not just caller-supplied parsed facts.
pub(crate) fn validate_page(
    c: &HistoryClaim,
    p: &HistoryPage,
    l: &HistoryLimits,
    now: u64,
) -> Result<()> {
    if p.dataset != c.dataset
        || p.canonical_revision != c.canonical_revision
        || p.history_epoch != c.epoch
        || p.verification != "PUBLIC_RPC_FINALIZED_TRANSACTIONS_META_RAW_V1"
        || p.historical_accounts_complete
        || p.genesis_hash != c.dataset.genesis_hash
        || p.source_fingerprint != c.dataset.source_fingerprint
        || p.head != c.head
        || p.from != c.from
        || p.to != c.to
        || p.from > p.to
        || p.to >= c.head.slot
        || p.to - p.from >= l.page_slots
        || p.head.timestamp_ms > now.saturating_add(30000)
        || p.raw.len() > l.page_slots as usize + 9
    {
        return Err(err("HISTORY_PAGE_IDENTITY_OR_FUTURE"));
    }
    let mut identities = 0;
    let mut heads = 0;
    let mut head_parent = None;
    let mut slots = 0;
    let mut enumeration = None;
    let mut successor_slot = None;
    let mut successor_headers = BTreeMap::new();
    let mut blocks = BTreeMap::new();
    let mut transferred = 0usize;
    for r in &p.raw {
        transferred = transferred
            .checked_add(r.response.len())
            .and_then(|n| n.checked_add(serde_json::to_vec(&r.params).ok()?.len()))
            .ok_or_else(|| err("HISTORY_RESPONSE_BUDGET"))?;
        if r.response.len() > l.max_response_bytes || transferred > l.max_batch_bytes {
            return Err(err("HISTORY_RESPONSE_BUDGET"));
        }
        let v = value(r)?;
        match r.method.as_str() {
            "getGenesisHash" => {
                if v.as_str() != Some(c.dataset.genesis_hash.as_str()) {
                    return Err(err("HISTORY_GENESIS_MISMATCH"));
                }
                identities += 1;
            }
            "getSlot" => {
                if r.params[0]["commitment"] != "finalized"
                    || v.as_u64().is_none_or(|s| s < c.head.slot)
                {
                    return Err(err("HISTORY_FINALIZED_HEAD_UNKNOWN"));
                }
                slots += 1;
            }
            "getBlocksWithLimit" => {
                if successor_slot.is_some()
                    || r.params[0] != p.to + 1
                    || r.params[1] != 1
                    || r.params[2]["commitment"] != "finalized"
                {
                    return Err(err("HISTORY_SUCCESSOR_UNKNOWN"));
                }
                let rows = v
                    .as_array()
                    .ok_or_else(|| err("HISTORY_SUCCESSOR_UNKNOWN"))?;
                if rows.len() != 1 {
                    return Err(err("HISTORY_SUCCESSOR_UNKNOWN"));
                }
                let slot = rows[0]
                    .as_u64()
                    .ok_or_else(|| err("HISTORY_SUCCESSOR_UNKNOWN"))?;
                if slot <= p.to || slot > p.head.slot {
                    return Err(err("HISTORY_SUCCESSOR_UNKNOWN"));
                }
                successor_slot = Some(slot);
            }
            "getBlocks" => {
                if enumeration.is_some()
                    || r.params[0] != p.from
                    || r.params[1] != p.to
                    || r.params[2]["commitment"] != "finalized"
                {
                    return Err(err("HISTORY_ENUMERATION"));
                }
                let rows = v.as_array().ok_or_else(|| err("HISTORY_ENUMERATION"))?;
                if rows.len() > l.page_slots as usize {
                    return Err(err("HISTORY_ENUMERATION"));
                }
                let mut out = Vec::new();
                for row in rows {
                    let slot = row.as_u64().ok_or_else(|| err("HISTORY_ENUMERATION"))?;
                    if slot < p.from || slot > p.to || out.last().is_some_and(|last| *last >= slot)
                    {
                        return Err(err("HISTORY_ENUMERATION"));
                    }
                    out.push(slot);
                }
                enumeration = Some(out);
            }
            "getBlock" => {
                let slot = r.params[0]
                    .as_u64()
                    .ok_or_else(|| err("HISTORY_BLOCK_HEADER"))?;
                if r.params[1]["commitment"] != "finalized" || r.params[1]["encoding"] != "json" {
                    return Err(err("HISTORY_FINALIZED_HEAD_UNKNOWN"));
                }
                let (h, parent, previous) = header(slot, &v, now)?;
                if slot == c.head.slot && r.params[1]["transactionDetails"] == "none" {
                    if h != c.head.hash
                        || previous != c.head.parent_hash
                        || head_parent.is_some_and(|p| p != parent)
                    {
                        return Err(err("HISTORY_CANONICAL_HEAD_CHANGED"));
                    }
                    head_parent = Some(parent);
                    heads += 1;
                } else if slot > p.to
                    && slot < c.head.slot
                    && r.params[1]["transactionDetails"] == "none"
                {
                    if successor_headers.insert(slot, (parent, previous)).is_some()
                        || successor_headers.len() > 1
                    {
                        return Err(err("HISTORY_SUCCESSOR_UNKNOWN"));
                    }
                } else {
                    let tx = v["transactions"]
                        .as_array()
                        .ok_or_else(|| err("HISTORY_TRANSACTIONS_UNKNOWN"))?;
                    if slot < p.from
                        || slot > p.to
                        || r.params[1]["transactionDetails"] != "full"
                        || tx.len() > 4096
                        || tx.iter().any(|t| {
                            !t["meta"].is_object()
                                || !t["transaction"].is_object()
                                || t["meta"].get("err").is_none()
                                || t["meta"]["fee"].as_u64().is_none()
                                || !t["transaction"]["message"].is_object()
                                || t["transaction"]["signatures"].as_array().is_none_or(|s| {
                                    s.is_empty()
                                        || s.iter().any(|v| {
                                            v.as_str().is_none_or(|s| {
                                                !bs58::decode(s)
                                                    .into_vec()
                                                    .is_ok_and(|b| b.len() == 64)
                                            })
                                        })
                                })
                        })
                    {
                        return Err(err("HISTORY_TRANSACTIONS_UNKNOWN"));
                    }
                    if blocks
                        .insert(
                            slot,
                            HistoryBlock {
                                slot,
                                hash: h,
                                parent_slot: parent,
                                parent_hash: previous,
                                transactions: v["transactions"].clone(),
                            },
                        )
                        .is_some()
                    {
                        return Err(err("HISTORY_DUPLICATE_BLOCK"));
                    }
                }
            }
            _ => return Err(err("HISTORY_RPC_METHOD")),
        }
    }
    if identities != 2 || heads != 2 || slots != 2 {
        return Err(err("HISTORY_IDENTITY_EVIDENCE_MISSING"));
    }
    let enumerated = enumeration.ok_or_else(|| err("HISTORY_ENUMERATION_MISSING"))?;
    if !blocks.keys().copied().eq(enumerated.iter().copied())
        || p.produced.len() != enumerated.len()
    {
        return Err(err("HISTORY_ENUMERATION_INCOMPLETE"));
    }
    if p.skipped != skipped_ranges(p.from, p.to, &enumerated) {
        return Err(err("HISTORY_SKIP_PROOF"));
    }
    let parent = head_parent.ok_or_else(|| err("HISTORY_FINALIZED_HEAD_UNKNOWN"))?;
    if parent >= p.from && parent <= p.to {
        if blocks
            .last_key_value()
            .is_none_or(|(slot, b)| *slot != parent || b.hash != p.head.parent_hash)
        {
            return Err(err("HISTORY_ENUMERATION_HEAD_PARENT"));
        }
    } else if parent < p.from && !blocks.is_empty() {
        return Err(err("HISTORY_ENUMERATION_HEAD_PARENT"));
    }
    if parent > p.to && blocks.last_key_value().is_none_or(|(slot, _)| *slot < p.to) {
        let slot = successor_slot.ok_or_else(|| err("HISTORY_COVERAGE_UNKNOWN"))?;
        let (parent, hash) = if slot == p.head.slot {
            (parent, p.head.parent_hash.clone())
        } else {
            successor_headers
                .get(&slot)
                .cloned()
                .ok_or_else(|| err("HISTORY_SUCCESSOR_UNKNOWN"))?
        };
        match blocks.last_key_value() {
            Some((last, block)) if parent == *last && hash == block.hash => {}
            None if parent < p.from => {}
            _ => return Err(err("HISTORY_COVERAGE_UNKNOWN")),
        }
    }
    let mut previous: Option<&HistoryBlock> = None;
    for (provided, slot) in p.produced.iter().zip(enumerated) {
        let b = &blocks[&slot];
        if provided.slot != b.slot
            || provided.hash != b.hash
            || provided.parent_slot != b.parent_slot
            || provided.parent_hash != b.parent_hash
            || provided.transactions != b.transactions
        {
            return Err(err("HISTORY_FACT_MISMATCH"));
        }
        if let Some(parent) = previous {
            if b.parent_slot != parent.slot || b.parent_hash != parent.hash {
                return Err(err("HISTORY_PARENT_CONFLICT"));
            }
        } else if b.slot != 0 && b.parent_slot >= p.from {
            return Err(err("HISTORY_ENUMERATION_PARENT_MISSING"));
        }
        previous = Some(b);
    }
    Ok(())
}
pub(crate) fn skipped_ranges(from: u64, to: u64, produced: &[u64]) -> Vec<xone_domain::Gap> {
    let mut ranges = Vec::new();
    let mut cursor = from;
    for slot in produced {
        if cursor < *slot {
            ranges.push(xone_domain::Gap {
                from: cursor,
                to: slot - 1,
                reason: "GETBLOCKS_FINALIZED_AT_BOUND_HEAD".into(),
            });
        }
        cursor = slot.saturating_add(1);
    }
    if cursor <= to {
        ranges.push(xone_domain::Gap {
            from: cursor,
            to,
            reason: "GETBLOCKS_FINALIZED_AT_BOUND_HEAD".into(),
        });
    }
    ranges
}
struct PublicSource {
    rpc: Rpc,
    url: String,
    limits: HistoryLimits,
    raw: Vec<RpcReceipt>,
    remaining: usize,
}
impl PublicSource {
    async fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let mut response = self
            .rpc
            .http
            .post(&self.url)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
            .send()
            .await
            .map_err(|_| err("HISTORY_SOURCE_UNREACHABLE"))?;
        if !response.status().is_success() {
            return Err(err("HISTORY_HTTP_STATUS"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| err("HISTORY_SOURCE_UNREACHABLE"))?
        {
            if bytes
                .len()
                .checked_add(chunk.len())
                .is_none_or(|n| n > self.limits.max_response_bytes)
                || chunk.len() > self.remaining
            {
                return Err(err("HISTORY_RESPONSE_BUDGET"));
            }
            self.remaining -= chunk.len();
            bytes.extend(chunk);
        }
        let r = RpcReceipt {
            method: method.into(),
            params,
            response: bytes,
        };
        let cost = serde_json::to_vec(&r.params)
            .map_err(|_| err("HISTORY_ENCODING"))?
            .len();
        self.remaining = self
            .remaining
            .checked_sub(cost)
            .ok_or_else(|| err("HISTORY_RESPONSE_BUDGET"))?;
        let v = value(&r)?;
        self.raw.push(r);
        Ok(v)
    }
    async fn fetch(mut self, c: &HistoryClaim) -> Result<HistoryPage> {
        let genesis = self.call("getGenesisHash", json!([])).await?;
        if genesis.as_str() != Some(&c.dataset.genesis_hash) {
            return Err(err("HISTORY_GENESIS_MISMATCH"));
        }
        let slot = self
            .call("getSlot", json!([{"commitment":"finalized"}]))
            .await?;
        if slot.as_u64().is_none_or(|s| s < c.head.slot) {
            return Err(err("HISTORY_FINALIZED_HEAD_UNKNOWN"));
        }
        let head_params = json!([c.head.slot,{"encoding":"json","transactionDetails":"none","rewards":false,"commitment":"finalized","maxSupportedTransactionVersion":0}]);
        let h = self.call("getBlock", head_params.clone()).await?;
        if h["blockhash"].as_str() != Some(&c.head.hash) {
            return Err(err("HISTORY_CANONICAL_HEAD_CHANGED"));
        }
        let enumerated = self
            .call("getBlocks", json!([c.from,c.to,{"commitment":"finalized"}]))
            .await?;
        let listed = enumerated
            .as_array()
            .ok_or_else(|| err("HISTORY_ENUMERATION"))?;
        if listed.len() > self.limits.page_slots as usize {
            return Err(err("HISTORY_ENUMERATION"));
        }
        let mut produced = Vec::new();
        let mut last = None;
        for row in listed {
            let slot = row.as_u64().ok_or_else(|| err("HISTORY_ENUMERATION"))?;
            if slot < c.from || slot > c.to || last.is_some_and(|p| p >= slot) {
                return Err(err("HISTORY_ENUMERATION"));
            }
            last = Some(slot);
            let b=self.call("getBlock",json!([slot,{"encoding":"json","transactionDetails":"full","rewards":false,"commitment":"finalized","maxSupportedTransactionVersion":0}])).await?;
            let (hash, parent_slot, parent_hash) = header(slot, &b, xone_observability::now_ms())?;
            produced.push(HistoryBlock {
                slot,
                hash,
                parent_slot,
                parent_hash,
                transactions: b["transactions"].clone(),
            });
        }
        if produced.last().is_none_or(|b| b.slot < c.to)
            && h["parentSlot"].as_u64().is_none_or(|p| p > c.to)
        {
            let next = self
                .call(
                    "getBlocksWithLimit",
                    json!([c.to+1,1,{"commitment":"finalized"}]),
                )
                .await?;
            let rows = next
                .as_array()
                .ok_or_else(|| err("HISTORY_SUCCESSOR_UNKNOWN"))?;
            if rows.len() != 1 {
                return Err(err("HISTORY_SUCCESSOR_UNKNOWN"));
            }
            let slot = rows[0]
                .as_u64()
                .ok_or_else(|| err("HISTORY_SUCCESSOR_UNKNOWN"))?;
            if slot <= c.to || slot > c.head.slot {
                return Err(err("HISTORY_SUCCESSOR_UNKNOWN"));
            }
            if slot != c.head.slot {
                self.call("getBlock",json!([slot,{"encoding":"json","transactionDetails":"none","rewards":false,"commitment":"finalized","maxSupportedTransactionVersion":0}])).await?;
            }
        }
        self.call("getBlock", head_params).await?;
        self.call("getSlot", json!([{"commitment":"finalized"}]))
            .await?;
        self.call("getGenesisHash", json!([])).await?;
        let slots = produced.iter().map(|b| b.slot).collect::<Vec<_>>();
        let page = HistoryPage {
            dataset: c.dataset.clone(),
            canonical_revision: c.canonical_revision,
            history_epoch: c.epoch.clone(),
            verification: "PUBLIC_RPC_FINALIZED_TRANSACTIONS_META_RAW_V1".into(),
            historical_accounts_complete: false,
            skipped: skipped_ranges(c.from, c.to, &slots),
            genesis_hash: c.dataset.genesis_hash.clone(),
            source_fingerprint: c.dataset.source_fingerprint.clone(),
            from: c.from,
            to: c.to,
            head: c.head.clone(),
            produced,
            raw: self.raw,
        };
        validate_page(c, &page, &self.limits, xone_observability::now_ms())?;
        Ok(page)
    }
}
/// One scheduler per Parser, shared by all its chains. A bounded step can
/// overshoot a duty-cycle window by one step; measured work is repaid before
/// another step. This is not a physical CPU/IO isolation or hard-realtime claim.
#[derive(Debug)]
pub struct HistoryScheduler {
    share: f64,
    next: Option<Instant>,
    pub last_work: Duration,
    pub cooldown: Duration,
}
impl HistoryScheduler {
    pub fn new(share: f64) -> Result<Self> {
        if !share.is_finite() || !(0.0..=0.25).contains(&share) {
            return Err(err("HISTORY_SHARE"));
        }
        Ok(Self {
            share: share.min(0.15),
            next: Some(Instant::now()),
            last_work: Duration::ZERO,
            cooldown: Duration::ZERO,
        })
    }
    pub fn delay(&self, now: Instant) -> Duration {
        if self.share == 0.0 {
            Duration::from_secs(1)
        } else {
            self.next
                .map_or(Duration::MAX, |next| next.saturating_duration_since(now))
        }
    }
    pub fn finish(&mut self, started: Instant, finished: Instant) {
        self.last_work = finished.saturating_duration_since(started);
        self.cooldown = Duration::try_from_secs_f64(
            self.last_work.as_secs_f64() * (1.0 / self.share.max(f64::MIN_POSITIVE) - 1.0),
        )
        .unwrap_or(Duration::MAX);
        self.next = finished.checked_add(self.cooldown);
    }
}
#[derive(Clone, Serialize)]
pub struct HistoryRuntimeStatus {
    pub max_worker_share: f64,
    pub closed: bool,
    pub last_work_ms: u64,
    pub cooldown_ms: u64,
    pub runtime_error: String,
    pub queue: Option<HistoryStatus>,
}
pub struct HistoryRuntime {
    queues: BTreeMap<String, HistoryQueue>,
    urls: BTreeMap<String, String>,
    rpc: Rpc,
    share: f64,
    enabled: bool,
    timeout: Duration,
    limits: HistoryLimits,
    status: Mutex<(u64, u64, String)>,
    closed: AtomicBool,
    started: AtomicBool,
    control: watch::Sender<u64>,
    task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
}
impl HistoryRuntime {
    pub fn new(
        config: &ConfigSnapshot,
        stores: &BTreeMap<String, Arc<Mutex<Store>>>,
        rpc: Rpc,
    ) -> Result<Arc<Self>> {
        let h = &config.config.history;
        if h.namespace != "history" || h.online_write_permission {
            return Err(err("HISTORY_NAMESPACE"));
        }
        let scheduler = HistoryScheduler::new(h.max_worker_share)?;
        let mut queues = BTreeMap::new();
        let mut urls = BTreeMap::new();
        let mut limits = HistoryLimits::default();
        limits.max_response_bytes = limits
            .max_response_bytes
            .min(config.config.journal.max_payload_bytes);
        limits.validate()?;
        for chain in &config.config.chains {
            let url = chain.rpc_urls.first().cloned().unwrap_or_default();
            let reg = HistoryRegistration::new(
                &chain.key,
                &chain.genesis_hash,
                &url,
                h.enabled,
                limits.clone(),
            )?;
            queues.insert(
                chain.key.clone(),
                HistoryQueue::open(stores[&chain.key].clone(), reg)?,
            );
            urls.insert(chain.key.clone(), url);
        }
        let (control, _) = watch::channel(0);
        Ok(Arc::new(Self {
            queues,
            urls,
            rpc,
            share: scheduler.share,
            enabled: h.enabled,
            timeout: Duration::from_millis(config.config.journal.request_timeout_ms.clamp(1, 5000)),
            limits,
            status: Mutex::new((0, 0, String::new())),
            closed: AtomicBool::new(false),
            started: AtomicBool::new(false),
            control,
            task: tokio::sync::Mutex::new(None),
        }))
    }
    pub async fn start(self: &Arc<Self>, stop: watch::Receiver<bool>) {
        let mut task = self.task.lock().await;
        if self.closed.load(Ordering::SeqCst)
            || *stop.borrow()
            || self.started.swap(true, Ordering::SeqCst)
        {
            return;
        }
        *task = Some(tokio::spawn(self.clone().run(stop)));
    }
    pub async fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.control.send_modify(|v| *v = v.wrapping_add(1));
        if let Some(task) = self.task.lock().await.take() {
            let _ = task.await;
        }
    }
    pub fn status(&self, key: &str) -> Result<HistoryRuntimeStatus> {
        let q = self.queues.get(key).ok_or_else(|| err("UNKNOWN_CHAIN"))?;
        let queue = q.status();
        let s = self.status.lock().map_err(|_| err("HISTORY_STATUS_LOCK"))?;
        Ok(HistoryRuntimeStatus {
            max_worker_share: self.share,
            closed: self.closed.load(Ordering::SeqCst),
            last_work_ms: s.0,
            cooldown_ms: s.1,
            runtime_error: queue
                .as_ref()
                .err()
                .map_or_else(|| s.2.clone(), |e| e.0.clone()),
            queue: queue.ok(),
        })
    }
    pub async fn pause(&self, key: &str, paused: bool) -> Result<()> {
        let q = self
            .queues
            .get(key)
            .ok_or_else(|| err("UNKNOWN_CHAIN"))?
            .clone();
        tokio::task::spawn_blocking(move || q.set_paused(paused))
            .await
            .map_err(|_| err("HISTORY_WORKER_JOIN"))??;
        self.control.send_modify(|v| *v = v.wrapping_add(1));
        Ok(())
    }
    async fn settle(
        &self,
        q: &HistoryQueue,
        claim: &HistoryClaim,
        cancel: bool,
        reason: &str,
    ) -> Result<()> {
        let deadline = tokio::time::Instant::now() + self.timeout;
        loop {
            let queue = q.clone();
            let job = claim.clone();
            let reason = reason.to_owned();
            let result = tokio::task::spawn_blocking(move || {
                let now = xone_observability::now_ms();
                if cancel {
                    queue.cancel(&job, now)
                } else {
                    queue.retry(&job, now, &reason)
                }
            })
            .await
            .map_err(|_| err("HISTORY_WORKER_JOIN"))?;
            if result.as_ref().is_err_and(|e| e.0 == "HISTORY_OWNER_BUSY")
                && tokio::time::Instant::now() < deadline
            {
                tokio::time::sleep(Duration::from_millis(1)).await;
                continue;
            }
            return result;
        }
    }
    async fn run(self: Arc<Self>, mut stop: watch::Receiver<bool>) {
        let mut scheduler = HistoryScheduler::new(self.share).unwrap();
        let mut changes = self.control.subscribe();
        let keys = self.queues.keys().cloned().collect::<Vec<_>>();
        let mut turn = 0;
        loop {
            if *stop.borrow() || stop.has_changed().is_err() || self.closed.load(Ordering::SeqCst) {
                break;
            }
            let delay = scheduler
                .delay(Instant::now())
                .max(Duration::from_millis(self.limits.poll_ms))
                .min(Duration::from_secs(1));
            tokio::select! {_=stop.changed()=>continue,_=changes.changed()=>continue,_=tokio::time::sleep(delay)=>{}}
            if !scheduler.delay(Instant::now()).is_zero()
                || !self.enabled
                || self.share == 0.0
                || keys.is_empty()
            {
                continue;
            }
            let key = &keys[turn % keys.len()];
            turn = turn.wrapping_add(1);
            let started = Instant::now();
            let q = self.queues[key].clone();
            let claim_q = q.clone();
            let claim =
                tokio::task::spawn_blocking(move || claim_q.claim(xone_observability::now_ms()))
                    .await;
            let claim = match claim {
                Ok(Ok(Some(c))) => c,
                Ok(Ok(None)) => {
                    scheduler.finish(started, Instant::now());
                    continue;
                }
                other => {
                    scheduler.finish(started, Instant::now());
                    if let Ok(mut s) = self.status.lock() {
                        s.2 = match other {
                            Ok(Err(e)) => e.0,
                            _ => "HISTORY_WORKER_JOIN".into(),
                        };
                    }
                    continue;
                }
            };
            let source = PublicSource {
                rpc: self.rpc.clone(),
                url: self.urls[key].clone(),
                limits: self.limits.clone(),
                raw: Vec::new(),
                remaining: self.limits.max_batch_bytes,
            };
            let mut cancelled = false;
            let deadline = tokio::time::Instant::now() + self.timeout;
            let mut outcome = tokio::select! {biased;_=stop.changed()=>{cancelled=true;Err(err("HISTORY_CANCELLED"))},_=changes.changed()=>{cancelled=true;Err(err("HISTORY_CANCELLED"))},r=tokio::time::timeout_at(deadline,source.fetch(&claim))=>r.unwrap_or_else(|_|Err(err("HISTORY_SOURCE_DEADLINE")))};
            if tokio::time::Instant::now() >= deadline {
                outcome = Err(err("HISTORY_SOURCE_DEADLINE"));
            }
            let result = if cancelled || self.closed.load(Ordering::SeqCst) {
                self.settle(&q, &claim, true, "HISTORY_CANCELLED").await
            } else {
                match outcome {
                    Ok(page) => {
                        let queue = q.clone();
                        let job = claim.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            queue.complete(&job, page, xone_observability::now_ms())
                        })
                        .await
                        .map_err(|_| err("HISTORY_WORKER_JOIN"))
                        .and_then(|r| r);
                        if let Err(e) = &result {
                            let _ = self.settle(&q, &claim, false, &e.0).await;
                        }
                        result
                    }
                    Err(e) => {
                        let _ = self.settle(&q, &claim, false, &e.0).await;
                        Err(e)
                    }
                }
            };
            scheduler.finish(started, Instant::now());
            if let Ok(mut s) = self.status.lock() {
                s.0 = scheduler.last_work.as_millis().min(u64::MAX as u128) as u64;
                s.1 = scheduler.cooldown.as_millis().min(u64::MAX as u128) as u64;
                s.2 = result.err().map_or_else(String::new, |e| e.0);
            }
        }
    }
}
