use axum::{extract::State, routing::post, Json, Router};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use xone_domain::{hash, scope_hash, Pool, Snapshot};
pub const AUTH: &str = "xone-execution-browser-test-only-2026";
#[derive(Clone)]
pub struct Bank {
    pub data: Arc<Value>,
    pub mode: Arc<Mutex<String>>,
    pub calls: Arc<Mutex<Vec<String>>>,
}
impl Bank {
    pub fn new() -> Self {
        Self {
            data: Arc::new(
                serde_json::from_slice(
                    &std::fs::read(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../test/contracts/meteora-execution-bank.json"
                    ))
                    .unwrap(),
                )
                .unwrap(),
            ),
            mode: Arc::new(Mutex::new("ready".into())),
            calls: Default::default(),
        }
    }
    pub async fn serve(&self) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handler = Router::new().route("/", post(rpc)).with_state(self.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, handler).await.unwrap();
        });
        (format!("http://{addr}"), task)
    }
    pub fn api(&self, rpc: &str) -> (xone_quote_api::Api, Snapshot) {
        let mut c = xone_config::load(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
            "dex-router",
        )
        .unwrap();
        c.token = AUTH.into();
        c.config.routing.max_state_age_ms = 315360000000;
        c.config.routing.max_splits = 1;
        c.config.routing.split_parts = 4;
        c.config.routing.quote_ttl_ms = 60000;
        c.config.api.quote_timeout_ms = 30000;
        c.config.api.requests_per_minute = 10000;
        c.config.chains[1].key = "solana:mainnet-beta".into();
        c.config.chains[1].genesis_hash = self.data["genesis"].as_str().unwrap().into();
        c.config.chains[1].rpc_urls = vec![rpc.into()];
        let mut amm_chain = c.config.chains[1].clone();
        amm_chain.key = "solana:amm-local".into();
        c.config.chains.push(amm_chain);
        let api = xone_quote_api::Api::new(c).unwrap();
        let p: Pool = serde_json::from_value(self.data["pool"].clone()).unwrap();
        let mut s: Snapshot =
            serde_json::from_str(include_str!("../../../../test/fixtures/evm/baseline.json"))
                .unwrap();
        s.reference.block.chain_key = p.chain_key.clone();
        s.reference.block.number_or_slot = p.meteora.as_ref().unwrap().slot;
        s.reference.block.timestamp_ms = p.meteora.as_ref().unwrap().timestamp * 1000;
        s.reference.block.hash = self.data["genesis"].as_str().unwrap().into();
        s.reference.block.parent_hash = self.data["blockhash"].as_str().unwrap().into();
        s.reference.owner_service = "chain-parser-rust".into();
        s.pools = vec![p];
        s.source = "CONTROLLED_ORIGINAL_SBF_EXECUTION_BANK".into();
        s.reference.scope_hash = scope_hash(&s.pools).unwrap();
        s.reference.adapter_set_hash = hash(&vec![s.pools[0].adapter_version.clone()]).unwrap();
        api.replica.install_baseline(s.clone()).unwrap();
        let f: Value = serde_json::from_slice(
            &std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test/contracts/pump-amm-sell-target.json"
            ))
            .unwrap(),
        )
        .unwrap();
        let mut amm = s.clone();
        let mut p: Pool = serde_json::from_value(f["cases"][33]["pool"].clone()).unwrap();
        p.chain_key = "solana:amm-local".into();
        amm.reference.block.chain_key = p.chain_key.clone();
        amm.reference.block.number_or_slot = p.pump.as_ref().unwrap().slot;
        amm.reference.block.timestamp_ms = p.pump.as_ref().unwrap().timestamp * 1000;
        amm.pools = vec![p];
        amm.reference.scope_hash = scope_hash(&amm.pools).unwrap();
        amm.reference.adapter_set_hash = hash(&vec![amm.pools[0].adapter_version.clone()]).unwrap();
        api.replica.install_baseline(amm).unwrap();
        (api, s)
    }
    pub fn request(&self) -> Value {
        let p = &self.data["pool"];
        json!({"chainKey":p["chain_key"],"kind":"EXACT_OUT","sellToken":p["token0"],"buyToken":p["token1"],"buyAmount":self.data["target"],"maxSellAmount":"1000000000000000","caller":self.data["caller"],"recipient":self.data["recipient"],"slippageBps":50,"verification":"FAST"})
    }
    pub fn preparation(&self) -> Value {
        json!({"sourceTokenAccount":self.data["sourceTokenAccount"],"destinationTokenAccount":self.data["destinationTokenAccount"]})
    }
}
async fn rpc(State(b): State<Bank>, Json(q): Json<Value>) -> Json<Value> {
    let method = q["method"].as_str().unwrap();
    b.calls.lock().unwrap().push(method.into());
    let mode = b.mode.lock().unwrap().clone();
    let d = &b.data;
    let mut slot = d["pool"]["meteora"]["slot"].as_u64().unwrap();
    if mode == "stale" {
        slot -= 1;
    }
    let result = match method {
        "getGenesisHash" => {
            if mode == "wrong_chain" {
                json!("wrong-chain")
            } else {
                d["genesis"].clone()
            }
        }
        "getMultipleAccounts" => {
            let values = q["params"][0]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| {
                    let key = id.as_str().unwrap();
                    let mut a = d["accounts"].get(key).cloned().unwrap_or(Value::Null);
                    if (mode == "missing_destination" && id == &d["destinationTokenAccount"])
                        || (mode == "missing_source" && id == &d["sourceTokenAccount"])
                    {
                        return Value::Null;
                    }
                    if !a.is_null() {
                        let mut raw = STANDARD.decode(a["data"][0].as_str().unwrap()).unwrap();
                        if mode == "changed_clock"
                            && key == "SysvarC1ock11111111111111111111111111111111"
                        {
                            raw[32] ^= 1;
                        }
                        if id == &d["sourceTokenAccount"] {
                            if mode == "frozen" {
                                raw[108] = 2;
                            }
                            if mode == "insufficient" {
                                raw[64..72].fill(0);
                            }
                            if mode == "wrong_owner" {
                                a["owner"] = json!("11111111111111111111111111111111");
                            }
                        }
                        if mode == "changed_pool" && id == &d["pool"]["pool_id"] {
                            raw[76] ^= 1;
                        }
                        if mode == "changed_program" && id == &d["pool"]["meteora"]["programdata"] {
                            raw[45] ^= 1;
                        }
                        if mode == "fee_balance" && id == &d["caller"] {
                            a["lamports"] = json!(0);
                        }
                        a["data"][0] = STANDARD.encode(raw).into();
                    }
                    a
                })
                .collect::<Vec<_>>();
            json!({"context":{"slot":slot},"value":values})
        }
        "getLatestBlockhash" => {
            json!({"context":{"slot":slot},"value":{"blockhash":d["blockhash"],"lastValidBlockHeight":999999999}})
        }
        "getFeeForMessage" => {
            json!({"context":{"slot":slot},"value":if mode=="expired_blockhash"{Value::Null}else{json!(5000)}})
        }
        _ => Value::Null,
    };
    Json(json!({"jsonrpc":"2.0","id":q["id"],"result":result}))
}
