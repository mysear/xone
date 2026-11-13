#![allow(dead_code)] // Different test targets exercise different protocol subsets.
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
    pub fn wsol(balance: Option<u64>) -> Self {
        let mut data: Value = serde_json::from_slice(
            &std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test/contracts/meteora-wsol-bank.json"
            ))
            .unwrap(),
        )
        .unwrap();
        if let Some(balance) = balance {
            let original = data["originalSourceTokenAccount"]
                .as_str()
                .unwrap()
                .to_string();
            let key = data["sourceTokenAccount"].as_str().unwrap().to_string();
            let mut account = data["accounts"][&original].clone();
            let mut raw = STANDARD
                .decode(account["data"][0].as_str().unwrap())
                .unwrap();
            let rent = u64::from_le_bytes(raw[113..121].try_into().unwrap());
            raw[64..72].copy_from_slice(&balance.to_le_bytes());
            account["data"][0] = STANDARD.encode(raw).into();
            account["lamports"] = json!(rent + balance);
            data["accounts"][key] = account;
        }
        Self {
            data: Arc::new(data),
            mode: Arc::new(Mutex::new("ready".into())),
            calls: Default::default(),
        }
    }
    pub fn token2022(index: usize) -> Self {
        let fixture: Value = serde_json::from_slice(
            &std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test/contracts/meteora-token2022-execution-bank.json"
            ))
            .unwrap(),
        )
        .unwrap();
        Self {
            data: Arc::new(fixture["cases"][index].clone()),
            mode: Arc::new(Mutex::new("ready".into())),
            calls: Default::default(),
        }
    }
    pub fn raydium(index: usize) -> Self {
        let f: Value = serde_json::from_slice(
            &std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test/contracts/raydium-execution-bank.json"
            ))
            .unwrap(),
        )
        .unwrap();
        Self {
            data: Arc::new(f["cases"][index].clone()),
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
        let (slot, timestamp) = if let Some(m) = &p.meteora {
            (m.slot, m.timestamp)
        } else {
            let r = p.raydium_cpmm.as_ref().unwrap();
            (r.bank_slot, r.bank_unix_timestamp)
        };
        s.reference.block.number_or_slot = slot;
        s.reference.block.timestamp_ms = timestamp * 1000;
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
        let reverse = self.data["zeroForOne"]
            .as_bool()
            .map(|v| !v)
            .unwrap_or_else(|| self.data.get("originalSourceTokenAccount").is_some());
        json!({"chainKey":p["chain_key"],"kind":"EXACT_OUT","sellToken":p[if reverse {"token1"}else{"token0"}],"buyToken":p[if reverse {"token0"}else{"token1"}],"buyAmount":self.data["target"],"maxSellAmount":"1000000000000000","caller":self.data["caller"],"recipient":self.data["recipient"],"slippageBps":50,"verification":"FAST"})
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
    let mut slot = d["pool"]["meteora"]["slot"]
        .as_u64()
        .or_else(|| d["pool"]["raydium_cpmm"]["bank_slot"].as_u64())
        .unwrap();
    if mode == "stale" {
        slot -= 1;
    }
    let result = match method {
        "simulateTransaction" => {
            let input = serde_json::to_vec(&json!({"bank":&*b.data,"rpc":q,"mode":mode})).unwrap();
            let mut child = tokio::process::Command::new("python3");
            child
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../test/tools/solana_prepare_simulation_worker.py"
                ))
                .kill_on_drop(true)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            let mut child = child.spawn().unwrap();
            let mut stdin = child.stdin.take().unwrap();
            use tokio::io::AsyncWriteExt;
            stdin.write_all(&input).await.unwrap();
            drop(stdin);
            let output =
                tokio::time::timeout(std::time::Duration::from_secs(12), child.wait_with_output())
                    .await
                    .unwrap()
                    .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            serde_json::from_slice(&output.stdout).unwrap()
        }
        "getMinimumBalanceForRentExemption" => {
            let size = q["params"][0].as_u64().unwrap();
            assert!(matches!(size, 165 | 182));
            json!((128 + size) * 6960)
        }
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
                    if mode=="temporary_occupied" && a.is_null() && key!=d["sourceTokenAccount"].as_str().unwrap() && key!=d["destinationTokenAccount"].as_str().unwrap() {a=json!({"owner":"11111111111111111111111111111111","executable":false,"lamports":1,"data":["","base64"]});}
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
                        if mode == "changed_token2022_program" && id == &d["pool"]["meteora"]["token2022"]["programdata"] {raw[45] ^= 1;}
                        if mode == "changed_token2022_mint" && id == &d["pool"]["token0"] {raw[258] ^= 1;}
                        if mode == "changed_epoch" && key == "SysvarC1ock11111111111111111111111111111111" {raw[16] ^= 1;}
                        if mode == "changed_pool" && id == &d["pool"]["pool_id"] {
                            raw[76] ^= 1;
                        }
                        if mode == "changed_program" && (id == &d["pool"]["meteora"]["programdata"] || id==&d["pool"]["raydium_cpmm"]["programdata"]) {
                            raw[45] ^= 1;
                        }
                        if mode == "wrap_balance" && id == &d["caller"] {
                            a["lamports"] = json!(2045000);
                        }
                        if mode == "rent_balance" && id == &d["caller"] {
                            a["lamports"] = json!(6000);
                        }
                        if mode == "changed_associated_program"
                            && key == "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
                        {
                            raw[0] ^= 1;
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
