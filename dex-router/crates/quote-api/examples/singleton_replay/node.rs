use super::{
    json, sign_transaction, sign_transaction_with_key, LocalSimulationBundle, Path, Pool,
    SnapshotRef, Value,
};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use xone_domain::{singleton::ZERO_ADDRESS, Amount};
use xone_simulation::evm_fixture::{FixtureAccount, StorageEntry, BEACON_ROOTS_ADDRESS};

#[derive(Clone, Copy)]
pub(super) enum Executable<'a> {
    Anvil(&'a OsStr),
    Geth(&'a OsStr),
}

const PREFIX_SENDER: &str = "0x2b5ad5c4795c026514f8317c7a215e218dccd6cf";
const BEACON_ROOTS_CODE: &str = "0x3373fffffffffffffffffffffffffffffffffffffffe14604d57602036146024575f5ffd5b5f35801560495762001fff810690815414603c575f5ffd5b62001fff01545f5260205ff35b5f5ffd5b62001fff42064281555f359062001fff015500";

pub(super) struct Node {
    endpoint: String,
    client: reqwest::Client,
    control: Option<(
        tokio::process::ChildStdin,
        Lines<BufReader<tokio::process::ChildStdout>>,
    )>,
    _process: tokio::process::Child,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fault {
    Gas,
    Logs,
    Balance,
    Reorg,
    IgnoreTraceIndex,
    BeaconRoot,
    BeaconStorage,
}

pub(super) struct Mined {
    node: Arc<Node>,
    deadline: u64,
    pub(super) signed: Value,
    transaction_index: u64,
    fault: Arc<Mutex<Option<Fault>>>,
    execution_reads: Arc<AtomicUsize>,
}

fn hex_quantity(value: &Amount) -> String {
    format!("0x{:x}", value.0)
}

fn number(value: &Value) -> u64 {
    u64::from_str_radix(value.as_str().unwrap().strip_prefix("0x").unwrap(), 16).unwrap()
}

fn field(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap().into()
}

impl Node {
    pub(super) async fn start(
        executable: Executable<'_>,
        dir: &Path,
        bundle: &LocalSimulationBundle,
        now: u64,
    ) -> Self {
        let env = &bundle.fixture.execution_environment;
        let mut alloc = bundle
            .fixture
            .accounts
            .iter()
            .map(|account| {
                let storage = account
                    .storage
                    .iter()
                    .map(|entry| (&entry.slot, &entry.value))
                    .collect::<BTreeMap<_, _>>();
                (
                    account.address.clone(),
                    json!({
                        "balance": hex_quantity(&account.balance),
                        "nonce": format!("0x{:x}", account.nonce),
                        "code": account.code, "storage": storage,
                    }),
                )
            })
            .collect::<BTreeMap<_, _>>();
        if matches!(executable, Executable::Geth(_)) {
            alloc.insert(
                PREFIX_SENDER.into(),
                json!({"balance":"0x1000000000000000000"}),
            );
            alloc.insert(
                BEACON_ROOTS_ADDRESS.into(),
                json!({"balance":"0x0","nonce":"0x1","code":BEACON_ROOTS_CODE}),
            );
        }
        let genesis = json!({
            "config": {
                "chainId": env.chain_id, "homesteadBlock": 0, "eip150Block": 0,
                "eip155Block": 0, "eip158Block": 0, "byzantiumBlock": 0,
                "constantinopleBlock": 0, "petersburgBlock": 0, "istanbulBlock": 0,
                "berlinBlock": 0, "londonBlock": 0, "mergeNetsplitBlock": 0, "terminalTotalDifficulty": 0,
                "shanghaiTime": 0, "cancunTime": 0,
            },
            "timestamp": format!("0x{:x}", now - 1),
            "gasLimit": format!("0x{:x}", env.gas_limit), "difficulty": "0x0",
            "baseFeePerGas": hex_quantity(&env.base_fee), "alloc": alloc,
        });
        let genesis_path = dir.join("genesis.json");
        std::fs::write(&genesis_path, serde_json::to_vec(&genesis).unwrap()).unwrap();
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        drop(socket);
        let log = std::fs::File::create(dir.join("anvil.log")).unwrap();
        let mut command = match executable {
            Executable::Anvil(bin) => {
                let mut command = tokio::process::Command::new(bin);
                command
                    .args(["--host", "127.0.0.1", "--port", &port.to_string()])
                    .args([
                        "--hardfork",
                        "cancun",
                        "--no-mining",
                        "--accounts",
                        "0",
                        "--init",
                    ])
                    .arg(genesis_path)
                    .stdout(log.try_clone().unwrap());
                command
            }
            Executable::Geth(bin) => {
                let mut command = tokio::process::Command::new(bin);
                command
                    .arg(genesis_path)
                    .arg(port.to_string())
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped());
                command
            }
        };
        let mut process = command.stderr(log).kill_on_drop(true).spawn().unwrap();
        let control = process.stdin.take().map(|stdin| {
            (
                stdin,
                BufReader::new(process.stdout.take().unwrap()).lines(),
            )
        });
        let node = Self {
            endpoint: format!("http://127.0.0.1:{port}"),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
            control,
            _process: process,
        };
        let request = json!({"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]});
        for _ in 0..100 {
            if let Ok(response) = node.forward(&request).await {
                assert_eq!(number(&response["result"]), env.chain_id);
                return node;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!(
            "Receipt node failed to start: {}",
            std::fs::read_to_string(dir.join("anvil.log")).unwrap()
        );
    }

    async fn forward(&self, request: &Value) -> Result<Value, reqwest::Error> {
        self.client
            .post(&self.endpoint)
            .json(request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
    }

    async fn rpc(&self, method: &str, params: Value) -> Value {
        let result = self
            .forward(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
            .await
            .unwrap();
        assert!(result.get("result").is_some(), "{method}: {result}");
        result["result"].clone()
    }

    async fn seal(&mut self, timestamp: u64) {
        if let Some((stdin, stdout)) = &mut self.control {
            stdin.write_all(b"mine\n").await.unwrap();
            let block = tokio::time::timeout(Duration::from_secs(10), stdout.next_line())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert!(block.starts_with("0x"), "{block}");
        } else {
            self.rpc("evm_setNextBlockTimestamp", json!([timestamp]))
                .await;
            self.rpc("evm_mine", json!([])).await;
        }
    }

    pub(super) async fn mine(
        mut self,
        case: &Value,
        bundle: &mut LocalSimulationBundle,
        reference: &mut SnapshotRef,
        pool: &mut Pool,
        now: u64,
    ) -> Mined {
        let genesis = self
            .rpc("eth_getBlockByNumber", json!(["0x0", false]))
            .await;
        self.seal(now).await;
        let source = self
            .rpc("eth_getBlockByNumber", json!(["0x1", false]))
            .await;
        let anchor = &mut bundle.fixture.state_anchor;
        anchor.genesis_hash = field(&genesis, "hash");
        anchor.block_hash = field(&source, "hash");
        anchor.parent_hash = field(&source, "parentHash");
        anchor.timestamp = number(&source["timestamp"]);
        anchor.block_number = number(&source["number"]);
        reference.block.hash = anchor.block_hash.clone();
        reference.block.parent_hash = anchor.parent_hash.clone();
        reference.block.number_or_slot = anchor.block_number;
        reference.block.timestamp_ms = anchor.timestamp * 1000;
        if let Some(state) = &mut pool.singleton_advanced {
            state.snapshot_block_hash = anchor.block_hash.clone();
        }
        if let Some(state) = &mut pool.infinity_bin {
            state.block_hash = anchor.block_hash.clone();
            state.block_number = anchor.block_number;
        }

        let deadline = now + 300;
        let mut data = field(case, "calldata");
        let input_limit = &data[data.len() - 64 * 4..data.len() - 64 * 3];
        let value = if case["context"]["sell_token"] == ZERO_ADDRESS {
            Amount::from_be_bytes(&hex::decode(input_limit).unwrap()).unwrap()
        } else {
            Amount::zero()
        };
        data.truncate(data.len() - 64);
        data.push_str(&format!("{deadline:064x}"));
        let signed = sign_transaction(
            &json!({"to":case["executor"],"data":data,"value":value}),
            bundle,
            bundle.transaction_context.nonce,
        );
        if self.control.is_some() {
            let mut prefix_context = bundle.clone();
            prefix_context.transaction_context.max_priority_fee_per_gas =
                Some("4".parse().unwrap());
            let prefix = sign_transaction_with_key(
                &json!({"to":"0x00000000000000000000000000000000000000ff","data":"0x","value":"1"}),
                &prefix_context,
                0,
                2,
            );
            self.rpc("eth_sendRawTransaction", json!([prefix["signedWire"]]))
                .await;
        }
        let tx = self
            .rpc("eth_sendRawTransaction", json!([signed["signedWire"]]))
            .await;
        assert_eq!(tx, signed["transactionHash"]);
        let transaction_index = if self.control.is_some() {
            let suffix = sign_transaction(
                &json!({"to":case["context"]["recipient"],"data":"0x","value":"1"}),
                bundle,
                bundle.transaction_context.nonce + 1,
            );
            self.rpc("eth_sendRawTransaction", json!([suffix["signedWire"]]))
                .await;
            1
        } else {
            self.rpc(
                "anvil_setNextBlockBaseFeePerGas",
                json!([hex_quantity(&bundle.fixture.execution_environment.base_fee)]),
            )
            .await;
            0
        };
        self.seal(now + 1).await;
        let execution = self
            .rpc("eth_getBlockByNumber", json!(["0x2", false]))
            .await;
        assert_eq!(
            execution["transactions"].as_array().unwrap().len(),
            if transaction_index == 0 { 1 } else { 3 }
        );
        assert_eq!(execution["transactions"][transaction_index as usize], tx);
        let receipt = self.rpc("eth_getTransactionReceipt", json!([tx])).await;
        assert_eq!(receipt["status"], "0x1", "{receipt}");
        let env = &mut bundle.fixture.execution_environment;
        env.block_hash = field(&execution, "hash");
        env.parent_hash = field(&execution, "parentHash");
        env.prevrandao = field(&execution, "mixHash");
        env.coinbase = field(&execution, "miner");
        env.timestamp = number(&execution["timestamp"]);
        env.block_number = number(&execution["number"]);
        env.gas_limit = number(&execution["gasLimit"]);
        env.base_fee = number(&execution["baseFeePerGas"])
            .to_string()
            .parse()
            .unwrap();
        env.excess_blob_gas = Some(number(&execution["excessBlobGas"]));
        if self.control.is_some() {
            env.parent_beacon_block_root = Some(field(&execution, "parentBeaconBlockRoot"));
            let source_anchor = json!({"blockHash":source["hash"],"requireCanonical":true});
            let mut slots = Vec::new();
            for slot in env.beacon_root_slots().unwrap().unwrap() {
                let value = self
                    .rpc(
                        "eth_getStorageAt",
                        json!([BEACON_ROOTS_ADDRESS, format!("0x{slot:x}"), source_anchor,]),
                    )
                    .await;
                slots.push(StorageEntry {
                    slot: format!("0x{slot:064x}"),
                    value: value.as_str().unwrap().into(),
                });
            }
            bundle.fixture.accounts.push(FixtureAccount {
                address: BEACON_ROOTS_ADDRESS.into(),
                exists: true,
                balance: Amount::zero(),
                nonce: 1,
                code: BEACON_ROOTS_CODE.into(),
                storage: slots,
            });
        }
        Mined {
            node: Arc::new(self),
            deadline,
            signed,
            transaction_index,
            fault: Arc::new(Mutex::new(None)),
            execution_reads: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl Mined {
    pub(super) fn transaction_index(&self) -> u64 {
        self.transaction_index
    }

    pub(super) fn scope(&self) -> &'static str {
        if self.transaction_index == 0 {
            "SINGLE_TRANSACTION_BLOCK"
        } else {
            "TRANSACTION_INDEX"
        }
    }
    pub(super) fn quote_ttl_ms(&self) -> u64 {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        // Allow the quote handler most of a second to encode the mined deadline.
        self.deadline * 1000 + 100 - now.as_millis() as u64
    }

    pub(super) fn inject(&self, fault: Fault) {
        self.execution_reads.store(0, Ordering::Relaxed);
        *self.fault.lock().unwrap() = Some(fault);
    }

    pub(super) fn router(&self) -> axum::Router {
        let node = self.node.clone();
        let fault = self.fault.clone();
        let execution_reads = self.execution_reads.clone();
        axum::Router::new().route(
            "/",
            axum::routing::post(move |axum::Json(mut request): axum::Json<Value>| {
                let (node, fault, execution_reads) = (
                    node.clone(),
                    *fault.lock().unwrap(),
                    execution_reads.clone(),
                );
                async move {
                    if fault == Some(Fault::IgnoreTraceIndex)
                        && request["method"] == "debug_traceCall"
                    {
                        request["params"][2]
                            .as_object_mut()
                            .unwrap()
                            .remove("txIndex");
                    }
                    let mut response = node.forward(&request).await.unwrap();
                    if let Some(error) = response.get("error") {
                        eprintln!("{}: {error}", request["method"]);
                    }
                    let result = &mut response["result"];
                    match (fault, request["method"].as_str().unwrap()) {
                        (Some(Fault::Gas), "eth_getTransactionReceipt") => {
                            result["gasUsed"] = "0x1".into();
                        }
                        (Some(Fault::Logs), "eth_getTransactionReceipt") => {
                            assert!(!result["logs"].as_array().unwrap().is_empty());
                            result["logs"][0]["data"] = "0x".into();
                        }
                        (Some(Fault::Balance), "eth_getBalance") => *result = "0x1".into(),
                        (Some(Fault::Balance), "debug_traceCall")
                            if request["params"][2]["tracer"] == "callTracer" =>
                        {
                            result["output"] = format!("0x{:064x}", 1).into();
                        }
                        (Some(Fault::Balance), "eth_call")
                            if request["params"][0]["data"]
                                .as_str()
                                .unwrap()
                                .starts_with("0x70a08231") =>
                        {
                            *result = format!("0x{:064x}", 1).into();
                        }
                        (Some(Fault::Reorg), "eth_getBlockByNumber")
                            if request["params"][0] == "0x2"
                                && execution_reads.fetch_add(1, Ordering::Relaxed) == 1 =>
                        {
                            result["hash"] = format!("0x{}", "01".repeat(32)).into();
                        }
                        (Some(Fault::BeaconRoot), "eth_getBlockByNumber")
                            if request["params"][0] == "0x2" =>
                        {
                            result["parentBeaconBlockRoot"] =
                                format!("0x{}", "01".repeat(32)).into();
                        }
                        (Some(Fault::BeaconStorage), "eth_getStorageAt") => {
                            *result = format!("0x{}", "02".repeat(32)).into();
                        }
                        _ => {}
                    }
                    axum::Json(response)
                }
            }),
        )
    }
}
