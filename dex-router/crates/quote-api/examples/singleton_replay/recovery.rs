use super::{json, node, LocalSimulationBundle, Pool, SnapshotRef, Value};
use alloy_primitives::{keccak256, U256};
use std::{ffi::OsString, path::PathBuf, time::SystemTime};
use tokio::io::AsyncReadExt;
use xone_domain::singleton::ZERO_ADDRESS;
use xone_simulation::evm_fixture::StorageEntry;

pub(super) async fn serve(mut args: impl Iterator<Item = OsString>) {
    let geth = args.next().expect("Geth executable");
    let case = args.next().expect("replay case");
    let directory = PathBuf::from(args.next().expect("fixture directory"));
    let stalled_simulation_marker = args.next().map(|flag| {
        assert_eq!(flag, "--stall-first-simulation");
        directory.join("stalled-simulation")
    });
    assert!(args.next().is_none(), "unexpected fixture arguments");
    let case: Value = serde_json::from_slice(&std::fs::read(case).unwrap()).unwrap();
    let mut bundle: LocalSimulationBundle = serde_json::from_value(case["bundle"].clone()).unwrap();
    let mut reference: SnapshotRef = serde_json::from_value(case["snapshot"].clone()).unwrap();
    let mut pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
    let context = &case["context"];
    assert_eq!(case["quoteKind"], "EXACT_IN");
    assert_eq!(pool.protocol, "uniswap-v4-advanced");
    assert_ne!(context["sell_token"], ZERO_ADDRESS);
    assert_ne!(context["buy_token"], ZERO_ADDRESS);
    declare_initialized_ticks(&mut bundle, &pool);
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let node = node::Node::start(
        node::Executable::Geth(&geth, false),
        &directory,
        &bundle,
        now,
    )
    .await;
    let mined = node
        .mine(&case, &mut bundle, &mut reference, &mut pool, now)
        .await;
    let bundle_path = directory.join("bundle.json");
    std::fs::write(&bundle_path, serde_json::to_vec(&bundle).unwrap()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let manifest = json!({
        "rpc":format!("http://{}", listener.local_addr().unwrap()),
        "bundle":bundle_path,"genesis":bundle.fixture.state_anchor.genesis_hash,
        "sourceStateRoot":mined.source_state_root,
        "executionContextBlockHash":bundle.fixture.execution_environment.block_hash,
        "reference":reference,"pool":pool,
        "executor":{"executor":case["executor"],"executor_code_sha256":case["executorCodeSha256"],
                    "token_code_sha256":case["tokenDigests"]},
        "quote":{"chainKey":pool.chain_key,"kind":"EXACT_IN","caller":context["caller"],
                 "recipient":context["recipient"],"sellToken":context["sell_token"],
                 "buyToken":context["buy_token"],"slippageBps":context["slippage_bps"],
                 "sellAmount":case["input"],"verification":"VERIFIED"},
        "expectedOutput":case["output"],
    });
    let manifest_path = directory.join("manifest.json");
    let staged = manifest_path.with_extension("tmp");
    std::fs::write(&staged, serde_json::to_vec(&manifest).unwrap()).unwrap();
    std::fs::rename(staged, manifest_path).unwrap();
    axum::serve(listener, mined.recovery_router(stalled_simulation_marker))
        .with_graceful_shutdown(async {
            let _ = tokio::io::stdin().read_u8().await;
        })
        .await
        .unwrap();
}

fn declare_initialized_ticks(bundle: &mut LocalSimulationBundle, pool: &Pool) {
    // The replay swaps within one tick. The Go baseline also walks initialized
    // bitmap entries and reads their packed liquidity words from PoolManager.
    let state = &pool.singleton_advanced.as_ref().unwrap().snapshot;
    let manager = bundle
        .fixture
        .accounts
        .iter_mut()
        .find(|account| account.address == state.manager)
        .unwrap();
    let mut key = [0; 64];
    hex::decode_to_slice(&state.pool_id[2..], &mut key[..32]).unwrap();
    key[63] = 6;
    let pool_slot = U256::from_be_bytes(keccak256(key).0);
    key[32..].copy_from_slice(&(pool_slot + U256::from(4)).to_be_bytes::<32>());
    for tick in &state.core.ticks {
        key[..32].fill(if tick.tick < 0 { 0xff } else { 0 });
        key[28..32].copy_from_slice(&tick.tick.to_be_bytes());
        let slot = format!("{:#x}", keccak256(key));
        let net = tick.liquidity_net.parse::<i128>().unwrap();
        let gross = tick.liquidity_gross.to_string().parse::<u128>().unwrap();
        let value = format!("0x{:032x}{gross:032x}", net as u128);
        if let Some(existing) = manager.storage.iter().find(|entry| entry.slot == slot) {
            assert_eq!(existing.value, value);
        } else {
            manager.storage.push(StorageEntry { slot, value });
        }
    }
}
