#![allow(dead_code)] // Different test targets exercise different protocol subsets.
use axum::{extract::State, routing::post, Json, Router};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use xone_domain::{hash, scope_hash, Pool, Snapshot};
pub const AUTH: &str = "xone-execution-browser-test-only-2026";
#[derive(Clone)]
pub struct Bank {
    pub data: Arc<Value>,
    pub mode: Arc<Mutex<String>>,
    pub calls: Arc<Mutex<Vec<String>>>,
    pub requests: Arc<Mutex<Vec<Value>>>,
    pub transactions: Arc<Mutex<BTreeMap<String, Value>>>,
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
            requests: Default::default(),
            transactions: Default::default(),
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
            requests: Default::default(),
            transactions: Default::default(),
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
            requests: Default::default(),
            transactions: Default::default(),
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
            requests: Default::default(),
            transactions: Default::default(),
        }
    }
    pub fn raydium_funding(index: usize) -> Self {
        let f: Value = serde_json::from_slice(
            &std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test/contracts/raydium-funding-bank.json"
            ))
            .unwrap(),
        )
        .unwrap();
        Self {
            data: Arc::new(f["cases"][index].clone()),
            mode: Arc::new(Mutex::new("ready".into())),
            calls: Default::default(),
            requests: Default::default(),
            transactions: Default::default(),
        }
    }
    pub fn orca_adaptive(index: usize) -> Self {
        static FIXTURE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        let f = FIXTURE.get_or_init(|| {
            serde_json::from_slice(
                &std::fs::read(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../test/contracts/orca-adaptive-execution-bank.json"
                ))
                .unwrap(),
            )
            .unwrap()
        });
        let mut data = f["cases"][index].clone();
        for (key, value) in f["shared_accounts"].as_object().unwrap() {
            if data["accounts"].get(key).is_none() {
                data["accounts"][key] = value.clone();
            }
        }
        Self {
            data: Arc::new(data),
            mode: Arc::new(Mutex::new("ready".into())),
            calls: Default::default(),
            requests: Default::default(),
            transactions: Default::default(),
        }
    }
    pub fn rayclmm_advanced(index: usize) -> Self {
        static FIXTURE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        let f = FIXTURE.get_or_init(|| {
            serde_json::from_slice(
                &std::fs::read(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../test/contracts/rayclmm-advanced-execution-bank.json"
                ))
                .unwrap(),
            )
            .unwrap()
        });
        let mut data = f["cases"][index].clone();
        for (key, value) in f["shared_accounts"].as_object().unwrap() {
            if data["accounts"].get(key).is_none() {
                data["accounts"][key] = value.clone();
            }
        }
        Self {
            data: Arc::new(data),
            mode: Arc::new(Mutex::new("ready".into())),
            calls: Default::default(),
            requests: Default::default(),
            transactions: Default::default(),
        }
    }
    pub fn rayclmm(index: usize) -> Self {
        static FIXTURE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        let f = FIXTURE.get_or_init(|| {
            serde_json::from_slice(
                &std::fs::read(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../test/contracts/rayclmm-execution-bank.json"
                ))
                .unwrap(),
            )
            .unwrap()
        });
        let mut data = f["cases"][index].clone();
        for (key, value) in f["shared_accounts"].as_object().unwrap() {
            if data["accounts"].get(key).is_none() {
                data["accounts"][key] = value.clone();
            }
        }
        Self {
            data: Arc::new(data),
            mode: Arc::new(Mutex::new("ready".into())),
            calls: Default::default(),
            requests: Default::default(),
            transactions: Default::default(),
        }
    }
    pub fn orca(index: usize) -> Self {
        static FIXTURE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        let f = FIXTURE.get_or_init(|| {
            serde_json::from_slice(
                &std::fs::read(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../test/contracts/orca-execution-bank.json"
                ))
                .unwrap(),
            )
            .unwrap()
        });
        let mut data = f["cases"][index].clone();
        for (key, value) in f["shared_accounts"].as_object().unwrap() {
            if data["accounts"].get(key).is_none() {
                data["accounts"][key] = value.clone();
            }
        }
        Self {
            data: Arc::new(data),
            mode: Arc::new(Mutex::new("ready".into())),
            calls: Default::default(),
            requests: Default::default(),
            transactions: Default::default(),
        }
    }
    pub fn pump_amm(index: usize) -> Self {
        let f: Value = serde_json::from_slice(
            &std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test/contracts/pump-amm-execution-bank.json"
            ))
            .unwrap(),
        )
        .unwrap();
        let mut data = f["cases"][index].clone();
        for (key, value) in f["shared_accounts"].as_object().unwrap() {
            data["accounts"][key] = value.clone();
        }
        Self {
            data: Arc::new(data),
            mode: Arc::new(Mutex::new("ready".into())),
            calls: Default::default(),
            requests: Default::default(),
            transactions: Default::default(),
        }
    }
    pub fn pump_curve(index: usize) -> Self {
        let f: Value = serde_json::from_slice(
            &std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test/contracts/pump-curve-execution-bank.json"
            ))
            .unwrap(),
        )
        .unwrap();
        let mut data = f["cases"][index].clone();
        for (key, value) in f["shared_accounts"].as_object().unwrap() {
            data["accounts"][key] = value.clone();
        }
        Self {
            data: Arc::new(data),
            mode: Arc::new(Mutex::new("ready".into())),
            calls: Default::default(),
            requests: Default::default(),
            transactions: Default::default(),
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
        self.api_with_manifest(rpc, None)
    }
    pub fn api_with_manifest(
        &self,
        rpc: &str,
        manifest: Option<&str>,
    ) -> (xone_quote_api::Api, Snapshot) {
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
        if let Some(path) = manifest {
            c.config.api.solana_runtime_manifest = path.into();
        }
        if manifest.is_some() {
            if let Ok(directory) = std::env::var("XONE_TEST_SUBMISSION_DIR") {
                c.config.api.transaction_submission_enabled = true;
                c.config.api.transaction_receipt_journal = std::path::Path::new(&directory)
                    .join(format!(
                        "{}.ndjson",
                        hash(&json!({"bank": &*self.data, "rpc": rpc})).unwrap()
                    ))
                    .to_str()
                    .unwrap()
                    .into();
                c.config.api.transaction_submission_chains =
                    vec![self.data["pool"]["chain_key"].as_str().unwrap().into()];
                c.config.api.transaction_max_pending = 8;
            }
        }
        let api = xone_quote_api::Api::new(c).unwrap();
        let p: Pool = serde_json::from_value(self.data["pool"].clone()).unwrap();
        let mut s: Snapshot =
            serde_json::from_str(include_str!("../../../../test/fixtures/evm/baseline.json"))
                .unwrap();
        s.reference.block.chain_key = p.chain_key.clone();
        let (slot, timestamp) = if let Some(m) = &p.meteora {
            (m.slot, m.timestamp)
        } else if let Some(r) = p.orca_whirlpool.as_ref() {
            (r.bank_slot, r.bank_unix_timestamp)
        } else if let Some(r) = p.raydium_clmm.as_ref() {
            (r.bank_slot, r.bank_unix_timestamp)
        } else if let Some(r) = p.raydium_cpmm.as_ref() {
            (r.bank_slot, r.bank_unix_timestamp)
        } else {
            let r = p.pump.as_ref().unwrap();
            (r.slot, r.timestamp)
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
    b.requests.lock().unwrap().push(q.clone());
    let mode = b.mode.lock().unwrap().clone();
    let d = &b.data;
    let mut slot = d["pool"]["meteora"]["slot"]
        .as_u64()
        .or_else(|| d["pool"]["raydium_cpmm"]["bank_slot"].as_u64())
        .or_else(|| d["pool"]["orca_whirlpool"]["bank_slot"].as_u64())
        .or_else(|| d["pool"]["raydium_clmm"]["bank_slot"].as_u64())
        .or_else(|| d["pool"]["pump"]["slot"].as_u64())
        .unwrap();
    if mode == "stale" {
        slot -= 1;
    }
    let result = match method {
        "getBlockHeight" => json!(if mode == "expire_after_send"
            && !b.transactions.lock().unwrap().is_empty()
        {
            1_000_000_000u64
        } else {
            1000
        }),
        "isBlockhashValid" => {
            json!({"context":{"slot":if mode.starts_with("preflight_") || mode=="simulation_mismatch" {slot+1}else{slot}},"value":mode!="expire_after_send"||b.transactions.lock().unwrap().is_empty()})
        }
        "sendTransaction" => {
            if mode == "preflight_advancing" {
                assert_eq!(q["params"][1]["minContextSlot"], slot + 4);
            }
            let encoded = q["params"][0].as_str().unwrap();
            let wire = STANDARD.decode(encoded).unwrap();
            let signature = bs58::encode(&wire[1..65]).into_string();
            if !b.transactions.lock().unwrap().contains_key(&signature) {
                if mode == "expire_after_send" {
                    b.transactions
                        .lock()
                        .unwrap()
                        .insert(signature.clone(), json!({"dropped":true}));
                } else {
                    let tx = wallet_worker(&b, "execute", encoded, &mode).await;
                    b.transactions.lock().unwrap().insert(signature.clone(), tx);
                }
            }
            if mode == "send_uncertain" {
                return Json(
                    json!({"jsonrpc":"2.0","id":q["id"],"error":{"code":-32000,"message":"controlled response lost after execution"}}),
                );
            }
            json!(signature)
        }
        "getSignatureStatuses" => {
            let values=q["params"][0].as_array().unwrap().iter().map(|s|{
                let ledger=b.transactions.lock().unwrap();let tx=ledger.get(s.as_str().unwrap());
                if let Some(tx)=tx.filter(|v|v["dropped"]!=true){json!({"slot":slot,"confirmations":Value::Null,"confirmationStatus":if mode=="submit_pending"{"processed"}else if mode=="submit_confirmed"{"confirmed"}else{"finalized"},"err":tx["meta"]["err"]})}else{Value::Null}
            }).collect::<Vec<_>>();
            json!({"context":{"slot":slot},"value":values})
        }
        "getTransaction" => {
            let mut tx = b
                .transactions
                .lock()
                .unwrap()
                .get(q["params"][0].as_str().unwrap())
                .cloned()
                .unwrap_or(Value::Null);
            if mode == "receipt_missing" {
                tx = Value::Null;
            }
            if mode == "receipt_mismatch" && !tx.is_null() {
                let values = tx["meta"]["postTokenBalances"].as_array_mut().unwrap();
                if let Some(value) = values.iter_mut().find(|v| v["owner"] == d["recipient"]) {
                    value["uiTokenAmount"]["amount"] = "0".into();
                }
            }
            tx
        }
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
            let mut response: Value = serde_json::from_slice(&output.stdout).unwrap();
            if mode == "preflight_advancing" {
                assert_eq!(q["params"][1]["minContextSlot"], slot + 3);
                response["context"]["slot"] = (slot + 4).into();
            }
            if mode == "simulation_mismatch" {
                response["context"]["slot"] = (slot + 1).into();
            }
            if mode == "preflight_simulation_stale" {
                response["context"]["slot"] = (slot - 1).into();
            }
            response
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
                    if key=="SysvarRent111111111111111111111111111111111" && a.is_null(){a=json!({"owner":"Sysvar1111111111111111111111111111111111111","executable":false,"lamports":1,"data":["MBsAAAAAAAAAAAAAAADwPzI=","base64"]});}

                    if (mode == "missing_destination" && id == &d["destinationTokenAccount"])
                        || (mode == "missing_source" && id == &d["sourceTokenAccount"])
                    {
                        return Value::Null;
                    }
                    if mode=="changed_orca_oracle" && id==&d["pool"]["orca_whirlpool"]["oracle"] {a=json!({"owner":"11111111111111111111111111111111","executable":false,"lamports":1,"data":["AA==","base64"]});}
                    if mode=="missing_rayclmm_extension" && id==&d["pool"]["raydium_clmm"]["bitmap_extension"]["address"] {return Value::Null;}
                    if mode=="missing_rayclmm_observation" && id==&d["pool"]["raydium_clmm"]["observation"] {return Value::Null;}
                    if mode=="missing_orca_adaptive_oracle" && id==&d["pool"]["orca_whirlpool"]["oracle"] {return Value::Null;}
                    if mode=="temporary_occupied" && id!=&d["pool"]["orca_whirlpool"]["oracle"] && a.is_null() && key!=d["sourceTokenAccount"].as_str().unwrap() && key!=d["destinationTokenAccount"].as_str().unwrap() {a=json!({"owner":"11111111111111111111111111111111","executable":false,"lamports":1,"data":["","base64"]});}
                    if !a.is_null() {
                        let mut raw = STANDARD.decode(a["data"][0].as_str().unwrap()).unwrap();
                        if mode == "changed_clock"
                            && key == "SysvarC1ock11111111111111111111111111111111"
                        {
                            raw[32] ^= 1;
                        }
                        if raw.len() >= 165 && (id == &d["sourceTokenAccount"] || (d["sourceTokenAccount"] == d["caller"] && id == &d["destinationTokenAccount"])) {
                            if mode == "frozen" {
                                raw[108] = 2;
                            }
                            if mode == "insufficient" {
                                if raw[109..113]==[1,0,0,0]{a["lamports"]=json!(u64::from_le_bytes(raw[113..121].try_into().unwrap()));}
                                raw[64..72].fill(0);
                            }
                            if mode == "wrong_owner" {
                                a["owner"] = json!("11111111111111111111111111111111");
                            }
                        }
                        if mode == "changed_token2022_program" && id == &d["pool"]["meteora"]["token2022"]["programdata"] {raw[45] ^= 1;}
                        if mode == "changed_token2022_mint" && id == &d["pool"]["token0"] {raw[258] ^= 1;}
                        if mode == "changed_epoch" && key == "SysvarC1ock11111111111111111111111111111111" {raw[16] ^= 1;}
                        if mode=="changed_pump_fee_program" && id==&d["pool"]["pump"]["fee_programdata"] {raw[45]^=1;}
                        if mode=="changed_rayclmm_extension" && id==&d["pool"]["raydium_clmm"]["bitmap_extension"]["address"] {raw[40]^=1;}
                        if mode=="changed_rayclmm_seed" && id==&d["pool"]["pool_id"] {raw[391]^=1;}
                        if mode=="changed_rayclmm_dynamic" && id==&d["pool"]["pool_id"] {raw[1096]^=1;}
                        if mode=="changed_rayclmm_dynamic_reserved" && id==&d["pool"]["pool_id"] {raw[1130]^=1;}
                        if mode=="changed_rayclmm_array" && id==&d["pool"]["raydium_clmm"]["tick_arrays"][0]["address"] {raw[44]^=1;}
                        if mode=="changed_rayclmm_order" && id==&d["pool"]["raydium_clmm"]["tick_arrays"][0]["address"] {raw[44+168+124]^=1;}
                        if mode=="changed_rayclmm_version" && id==&d["pool"]["pool_id"] {raw[273]^=1;}
                        if mode=="changed_rayclmm_observation" && id==&d["pool"]["raydium_clmm"]["observation"] {raw[19]^=1;}
                        if mode=="changed_rayclmm_config" && id==&d["pool"]["raydium_clmm"]["amm_config"] {raw[47]^=1;}
                        if mode=="changed_rayclmm_mint" && id==&d["pool"]["token0"] {raw[44]^=1;}
                        if mode=="underbacked_rayclmm_vault" && id==&d["pool"]["raydium_clmm"]["vault0"] {a["lamports"]=json!(u64::from_le_bytes(raw[113..121].try_into().unwrap())+u64::from_le_bytes(raw[64..72].try_into().unwrap())-1);}
                        if mode=="changed_rayclmm_budget" && key=="ComputeBudget111111111111111111111111111111" {raw[0]^=1;}
                        if mode=="changed_rayclmm_token2022_program" {
                            let token_program=STANDARD.decode(d["accounts"]["TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"]["data"][0].as_str().unwrap()).unwrap();
                            if key==xone_domain::meteora::key(&token_program,4){raw[45]^=1;}
                        }
                        if mode=="changed_rayclmm_memo" && id=="MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr" {raw[0]^=1;}
                        if id==&d["pool"]["orca_whirlpool"]["oracle"] {
                            if mode=="changed_orca_adaptive_filter" {raw[48]^=1;}
                            if mode=="changed_orca_adaptive_reserved" {raw[253]^=1;}
                            if mode=="changed_orca_adaptive_binding" {raw[8]^=1;}
                        }
                        if mode == "changed_orca_array" && id == &d["pool"]["orca_whirlpool"]["tick_arrays"][0]["address"] { raw[12] ^= 1; }
                        if mode == "changed_orca_config" && id == &d["pool"]["orca_whirlpool"]["whirlpools_config"] {raw[106] = 2;}
                        if mode == "changed_orca_mint" && id == &d["pool"]["token0"] {raw[44] ^= 1;}
                        if mode == "underbacked_orca_vault" && id == &d["pool"]["orca_whirlpool"]["vault0"] {a["lamports"]=json!(u64::from_le_bytes(raw[113..121].try_into().unwrap())+u64::from_le_bytes(raw[64..72].try_into().unwrap())-1);}
                        if mode == "changed_orca_budget" && key == "ComputeBudget111111111111111111111111111111" {raw[0] ^= 1;}
                        if mode == "changed_pool" && id == &d["pool"]["pool_id"] {
                            raw[76] ^= 1;
                        }
                        if mode == "changed_program" && (id == &d["pool"]["raydium_clmm"]["programdata"] || id == &d["pool"]["orca_whirlpool"]["programdata"] || id == &d["pool"]["meteora"]["programdata"] || id==&d["pool"]["raydium_cpmm"]["programdata"] || id==&d["pool"]["pump"]["programdata"]) {
                            raw[45] ^= 1;
                        }
                        if mode == "wrap_balance" && id == &d["caller"] {
                            a["lamports"] = json!(2045000);
                        }
                        if mode == "rent_balance" && id == &d["caller"] {
                            a["lamports"] = json!(6000);
                        }
                        if mode=="changed_system_program" && key=="11111111111111111111111111111111" {a["owner"]=json!("11111111111111111111111111111111");}
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
            let observed = if mode == "preflight_advancing" {
                assert_eq!(q["params"][1]["minContextSlot"], slot + 2);
                slot + 3
            } else if mode == "preflight_account_stale" {
                slot - 1
            } else if mode == "preflight_simulation_stale" || mode == "simulation_mismatch" {
                slot + 1
            } else {
                slot
            };
            json!({"context":{"slot":observed},"value":values})
        }
        "getLatestBlockhash" => {
            json!({"context":{"slot":slot},"value":{"blockhash":d["blockhash"],"lastValidBlockHeight":999999999}})
        }
        "getFeeForMessage" => {
            if mode == "preflight_advancing" {
                assert_eq!(q["params"][1]["minContextSlot"], slot + 1);
            }
            let observed = match mode.as_str() {
                "fee_context_ahead" => slot + 1,
                "preflight_advancing" => slot + 2,
                "preflight_account_stale"
                | "preflight_simulation_stale"
                | "preflight_fee_changed"
                | "simulation_mismatch" => slot + 1,
                "preflight_fee_stale" => slot - 1,
                _ => slot,
            };
            let mut response = json!({"context":{"slot":observed},"value":if mode=="expired_blockhash"{Value::Null}else if mode=="preflight_fee_changed"{json!(5001)}else{json!(5000)}});
            if mode == "preflight_fee_missing" {
                response.as_object_mut().unwrap().remove("context");
            }
            if mode == "preflight_fee_string" {
                response["context"]["slot"] = observed.to_string().into();
            }
            response
        }
        _ => Value::Null,
    };
    if mode == "ready"
        && d["pool"]["protocol"] == "raydium-cpmm-creator"
        && d.get("fundingScenario").is_some()
    {
        if let Ok(folder) = std::env::var("XONE_RAYDIUM_FUNDING_RPC_DIR") {
            let record = json!({"scenario":d["fundingScenario"],"chainKey":d["pool"]["chain_key"],"genesisHash":d["genesis"],"request":q,"response":{"jsonrpc":"2.0","id":q["id"],"result":result}});
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(
                std::path::Path::new(&folder).join(format!(
                    "{}-{}.json",
                    method,
                    hash(&record).unwrap()
                )),
                serde_json::to_vec(&record).unwrap(),
            )
            .unwrap();
        }
    }
    if mode == "ready"
        && d.get("executionScenario").is_some()
        && xone_domain::raydium_clmm::supports_classic_profile(
            d["pool"]["protocol"].as_str().unwrap(),
        )
    {
        if let Ok(folder) = std::env::var(
            if d["pool"]["raydium_clmm"]["bitmap_extension"].is_object() {
                "XONE_RAYCLMM_ADVANCED_EXECUTION_RPC_DIR"
            } else {
                "XONE_RAYCLMM_EXECUTION_RPC_DIR"
            },
        ) {
            let record = json!({"scenario":d["executionScenario"],"chainKey":d["pool"]["chain_key"],"genesisHash":d["genesis"],"request":q,"response":{"jsonrpc":"2.0","id":q["id"],"result":result}});
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(
                std::path::Path::new(&folder).join(format!(
                    "{}-{}.json",
                    method,
                    hash(&record).unwrap()
                )),
                serde_json::to_vec(&record).unwrap(),
            )
            .unwrap();
        }
    }

    if mode == "ready"
        && d.get("executionScenario").is_some()
        && xone_domain::orca::supports_classic_profile(d["pool"]["protocol"].as_str().unwrap())
    {
        let variable = if d["pool"]["orca_whirlpool"]["adaptive"].is_object() {
            "XONE_ORCA_ADAPTIVE_EXECUTION_RPC_DIR"
        } else {
            "XONE_ORCA_EXECUTION_RPC_DIR"
        };
        if let Ok(folder) = std::env::var(variable) {
            let record = json!({"scenario":d["executionScenario"],"chainKey":d["pool"]["chain_key"],"genesisHash":d["genesis"],"request":q,"response":{"jsonrpc":"2.0","id":q["id"],"result":result}});
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(
                std::path::Path::new(&folder).join(format!(
                    "{}-{}.json",
                    method,
                    hash(&record).unwrap()
                )),
                serde_json::to_vec(&record).unwrap(),
            )
            .unwrap();
        }
    }
    Json(json!({"jsonrpc":"2.0","id":q["id"],"result":result}))
}

pub async fn wallet_worker(bank: &Bank, operation: &str, transaction: &str, mode: &str) -> Value {
    use tokio::io::AsyncWriteExt;
    let mut command = tokio::process::Command::new("python3");
    command
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/tools/solana_wallet_worker.py"
        ))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(&serde_json::to_vec(&json!({"bank":&*bank.data,"operation":operation,"transaction":transaction,"mode":mode})).unwrap()).await.unwrap();
    drop(stdin);
    let output = tokio::time::timeout(std::time::Duration::from_secs(12), child.wait_with_output())
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
