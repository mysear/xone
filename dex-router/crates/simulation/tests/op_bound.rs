#[path = "support/op_bound.rs"]
mod bank;
#[path = "support/op_user.rs"]
mod signing;
use revm::primitives::{keccak256, Address, B256, U256};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, str::FromStr};
use xone_domain::Pool;
use xone_execution_plan::{BoundEvmExactOutExecution, BoundEvmExecution};
use xone_simulation::revm_backend as core;
use xone_simulation::{evm_fixture::*, op_transaction::authenticate_user, revm_backend::OpFork};
fn original() -> Value {
    serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_OP_EXECUTION_VECTORS").unwrap()).unwrap(),
    )
    .unwrap()
}
fn fork(s: &str) -> OpFork {
    match s {
        "regolith" => OpFork::Regolith,
        "canyon" => OpFork::Canyon,
        "ecotone" => OpFork::Ecotone,
        "fjord" => OpFork::Fjord,
        "granite" => OpFork::Granite,
        "holocene" => OpFork::Holocene,
        "isthmus" => OpFork::Isthmus,
        "jovian" => OpFork::Jovian,
        "karst" => OpFork::Karst,
        _ => panic!(),
    }
}
fn account(address: &str) -> FixtureAccount {
    FixtureAccount {
        address: address.into(),
        exists: true,
        balance: "111".parse().unwrap(),
        nonce: 0,
        code: "0x".into(),
        storage: vec![],
    }
}
fn slot(n: U256) -> String {
    format!("0x{n:064x}")
}
fn fix(c: &Value, rule: &Value) -> Value {
    let mut c = c.clone();
    let time = rule["timestamp"].as_u64().unwrap();
    c["pool"]["chain_key"] = "evm:10".into();
    c["state_anchor"]["chain_key"] = "evm:10".into();
    c["transaction"]["chain_key"] = "evm:10".into();
    c["state_anchor"]["timestamp"] = (time - 2).into();
    c["execution_environment"]["timestamp"] = time.into();
    c["execution_environment"]["chain_id"] = 10.into();
    c["execution_environment"]["fork"] = format!("op-{}", rule["rule"].as_str().unwrap()).into();
    c["pool"]["v2"]["swap"]["bank_unix_timestamp"] = (time - 2).into();
    c["pool"]["v2"]["swap"]["block_timestamp_last"] = (time - 4).into();
    let pair = c["pool"]["pool_id"].as_str().unwrap().to_owned();
    for a in c["accounts"].as_array_mut().unwrap() {
        if a["address"] == pair {
            for s in a["storage"].as_array_mut().unwrap() {
                if s["slot"] == slot(U256::from(8)) {
                    let packed =
                        U256::from_str_radix(&s["value"].as_str().unwrap()[2..], 16).unwrap();
                    s["value"] =
                        slot((packed & (U256::MAX >> 32)) | (U256::from(time - 4) << 224)).into();
                }
            }
        }
    }
    c
}
struct Case {
    bound: BoundEvmExecution,
    exact: Option<BoundEvmExactOutExecution>,
    pools: Vec<Pool>,
    fixture: FixtureState,
    op: OpTransactionContext,
    wire: Vec<u8>,
    id: String,
}
fn make(c: &Value, r: &Value, kind: u8, exact: bool) -> Case {
    let c = fix(c, &r["input"]);
    let (b, p, mut f, t) = bank::bound(&c);
    let x = exact.then(|| {
        bank::output_bound(
            &b,
            &p,
            b.plan.allocation.gross_buy_amount.clone(),
            b.plan.allocation.sell_amount.clone(),
        )
    });
    let tx = x.as_ref().map_or(&b.transaction, |x| &x.transaction);
    let a = hex::decode(r["input"]["attributes"].as_str().unwrap()).unwrap();
    let mut l1 = account("0x4200000000000000000000000000000000000015");
    let mut put = |n: u64, bytes: &[u8]| {
        l1.storage.push(StorageEntry {
            slot: slot(U256::from(n)),
            value: slot(U256::from_be_slice(bytes)),
        })
    };
    if a.len() == 260 {
        put(1, &a[68..100]);
        put(5, &a[196..228]);
        put(6, &a[228..260]);
    } else {
        put(1, &a[36..68]);
        put(7, &a[68..100]);
        let mut s = [0; 32];
        s[16..24].copy_from_slice(&a[4..12]);
        put(3, &s);
        if a.len() >= 176 {
            let mut s = [0; 32];
            s[20..32].copy_from_slice(&a[164..176]);
            put(8, &s);
        }
    }
    f.accounts.push(l1);
    for suffix in ["19", "1a", "1b"] {
        f.accounts.push(account(&format!(
            "0x42000000000000000000000000000000000000{suffix}"
        )));
    }
    let mut fields = vec![signing::number(U256::from(t.nonce))];
    if kind == 2 {
        fields.push(signing::number(U256::from(2_500_000_000u64)));
    }
    fields.extend([
        signing::number(U256::from(3_000_000_000u64)),
        signing::number(U256::from(t.gas_limit)),
        signing::encoded(Address::from_str(&tx.to).unwrap().as_slice(), false),
        signing::number(U256::ZERO),
        signing::encoded(&hex::decode(&tx.data[2..]).unwrap(), false),
    ]);
    if kind > 0 {
        fields.insert(0, signing::number(U256::from(10)));
        fields.push(signing::encoded(&[], true));
    }
    let wire = signing::signed(kind, &fields);
    let transaction = authenticate_user(
        10,
        signing::hash(&wire),
        signing::payer(),
        &wire,
        wire.len(),
    )
    .unwrap();
    Case {
        bound: b,
        exact: x,
        pools: p,
        fixture: f,
        op: OpTransactionContext {
            fork: fork(r["input"]["rule"].as_str().unwrap()),
            transaction,
            attributes_calldata: a,
        },
        wire,
        id: format!(
            "{}-{}-{kind}-{}",
            c["name"].as_str().unwrap(),
            r["input"]["rule"].as_str().unwrap(),
            if exact { "out" } else { "in" }
        ),
    }
}
fn run(c: &Case, b: &SimulationBudget) -> Result<SimulationOutcome> {
    let allowed = vec![c.bound.executor.target.clone()];
    match &c.exact {
        Some(x) => simulate_bound_op_user_exact_out(
            x,
            &c.pools,
            &allowed,
            &c.fixture,
            &c.op,
            b,
            Default::default(),
        ),
        None => simulate_bound_op_user(
            &c.bound,
            &c.pools,
            &allowed,
            &c.fixture,
            &c.op,
            b,
            Default::default(),
        ),
    }
}
fn matrix(mut f: impl FnMut(Case)) {
    let v = original();
    for rule in [
        "regolith", "canyon", "ecotone", "fjord", "granite", "holocene", "isthmus",
    ] {
        let r = v["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["input"]["rule"] == rule && r["input"]["chain"] == 10)
            .unwrap();
        for c in bank::corpus()["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["service_eligible"] == true)
        {
            for kind in [0, 2] {
                for exact in [false, true] {
                    f(make(c, r, kind, exact));
                }
            }
        }
    }
}
#[test]
#[ignore = "generates complete immutable original-client inputs before acceptance"]
fn export_original_op_bound_inputs() {
    let mut rows = Vec::new();
    matrix(|c| {
        rows.push(json!({"id":c.id,"rule":format!("{:?}",c.op.fork).to_ascii_lowercase(),"fixture":c.fixture,"signed_wire":hex::encode(c.wire),"transaction_hash":format!("{:#x}",c.op.transaction.transaction_hash()),"caller":c.bound.transaction.from,"token_in":c.bound.plan.context.sell_token,"token_out":c.bound.plan.context.buy_token,"recipient":c.bound.plan.context.recipient}));
    });
    assert_eq!(rows.len(), 448);
    std::fs::write(
        std::env::var("XONE_OP_BOUND_INPUTS").unwrap(),
        serde_json::to_vec_pretty(&rows).unwrap(),
    )
    .unwrap();
}
fn complete_state(c: &Case, o: &SimulationOutcome, r: &Value) {
    let b = bank::budget();
    let image = c.fixture.to_image(&b).unwrap();
    let env = &c.fixture.execution_environment;
    let direct = core::execute_op_user(
        image.clone(),
        core::OpExecutionRequest {
            fork: c.op.fork,
            transaction: c.op.transaction.clone(),
            attributes_calldata: c.op.attributes_calldata.clone(),
            block: revm::primitives::BlockEnv {
                number: U256::from(env.block_number),
                coinbase: Address::from_str(&env.coinbase).unwrap(),
                timestamp: U256::from(env.timestamp),
                gas_limit: U256::from(env.gas_limit),
                basefee: U256::from_str(&env.base_fee.to_string()).unwrap(),
                prevrandao: Some(B256::from_str(&env.prevrandao).unwrap()),
                blob_excess_gas_and_price: Some(revm::primitives::BlobExcessGasAndPrice::new(
                    0, false,
                )),
                ..Default::default()
            },
        },
        core::ExecutionLimits {
            reads: core::BoundReadLimits {
                max_reads: b.max_reads,
                max_bytes: b.max_read_bytes,
            },
            max_gas_limit: b.max_transaction_gas,
            max_instructions: b.max_instructions,
            max_input_bytes: b.max_input_bytes,
            max_memory_bytes: b.max_memory_bytes,
        },
        Default::default(),
    )
    .unwrap();
    assert_eq!(
        direct.execution.result_and_state.result.gas_used(),
        o.execution.gas_used
    );
    // The BOUND receipt commits to exactly the delta whose complete declared
    // bank is compared below; probes cannot substitute a different transition.
    let state = &direct.execution.result_and_state.state;
    let mut rows = BTreeMap::new();
    for (addr, a) in state {
        let storage=a.storage.iter().map(|(slot,v)|(format!("{slot:064x}"),json!({"original":format!("{:064x}",v.original_value()),"present":format!("{:064x}",v.present_value())}))).collect::<BTreeMap<_,_>>();
        rows.insert(format!("{addr:#x}"),json!({"balance":a.info.balance.to_string(),"nonce":a.info.nonce,"code_hash":format!("{:#x}",a.info.code_hash),"status":a.status.bits(),"storage":storage}));
    }
    assert_eq!(
        hex::encode(Sha256::digest(serde_json::to_vec(&rows).unwrap())),
        o.execution.post_state_digest,
        "{} delta",
        c.id
    );
    let after = image.apply(state).unwrap();
    let d = after.data();
    assert_eq!(d.accounts.len(), r["accounts"].as_object().unwrap().len());
    for (a, info) in &d.accounts {
        let original = &r["accounts"][format!("{a:#x}")];
        assert!(original.is_object());
        let (balance, nonce, code) = info.as_ref().map_or((U256::ZERO, 0, "0x".into()), |i| {
            (
                i.balance,
                i.nonce,
                if i.code_hash == revm::primitives::KECCAK_EMPTY {
                    "0x".into()
                } else {
                    format!("0x{}", hex::encode(&d.bytecodes[&i.code_hash]))
                },
            )
        });
        assert_eq!(
            balance.to_string(),
            original["balance"].as_str().unwrap(),
            "{} {a} balance",
            c.id
        );
        assert_eq!(
            nonce,
            original["nonce"].as_u64().unwrap(),
            "{} {a} nonce",
            c.id
        );
        assert_eq!(
            code,
            original["code"].as_str().unwrap(),
            "{} {a} code",
            c.id
        );
    }
    for ((a, s), v) in &d.storage {
        assert_eq!(
            slot(*v),
            r["accounts"][format!("{a:#x}")]["storage"][slot(*s)]
                .as_str()
                .unwrap(),
            "{} {a} {s} storage",
            c.id
        );
    }
}
#[test]
#[ignore = "requires original-client full Router OP transition reference"]
fn bound_signed_op_router_measures_real_tokens_complete_state_and_all_chain_fees() {
    let v: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_OP_BOUND_VECTORS").unwrap()).unwrap(),
    )
    .unwrap();
    let rows = v["cases"].as_array().unwrap();
    let mut n = 0;
    matrix(|c| {
        let r = rows.iter().find(|r| r["id"] == c.id).unwrap();
        let o = run(&c, &bank::budget()).unwrap_or_else(|e| panic!("{} {e}", c.id));
        complete_state(&c, &o, r);
        assert!(o.matches_plan, "{} {:?}", c.id, o.execution);
        assert!(!o.verified);
        assert!(o.native_fees.is_none());
        assert_eq!(
            o.execution.gas_used,
            r["gas_used"].as_u64().unwrap(),
            "{}",
            c.id
        );
        assert_eq!(
            o.execution.sender_nonce_after,
            r["nonce_after"].as_u64().unwrap()
        );
        assert_eq!(
            o.actual_input_consumed.to_string(),
            r["spent"].as_str().unwrap()
        );
        assert_eq!(
            o.actual_buy_amount.to_string(),
            r["bought"].as_str().unwrap()
        );
        let fees = o.op_fees.as_ref().unwrap();
        assert_eq!(fees["transaction_hash"], r["transaction_hash"]);
        assert_eq!(fees["chain_cost"]["l1_data_fee"], r["l1_fee"]);
        assert_eq!(fees["chain_cost"]["operator_fee"], r["operator_fee"]);
        for (address, balances) in fees["accounts"].as_object().unwrap() {
            assert_eq!(
                balances["after"], r["accounts"][address]["balance"],
                "{} {address}",
                c.id
            );
        }
        for ob in o
            .observations
            .iter()
            .filter(|ob| ob.observation_type.is_some() && ob.phase.ends_with("after"))
        {
            let value = &r["accounts"][&ob.target]["storage"][ob.slot.as_ref().unwrap()];
            assert_eq!(&ob.output, value, "{} {}", c.id, ob.phase);
        }
        assert_eq!(o.simulator_version, "provided-local-state/op-signed-v1");
        assert!(o.usage.executions > 10);
        n += 1;
    });
    assert_eq!(n, 448);
}
#[test]
#[ignore = "requires fixed OP parameters; no oracle needed for fail-closed cases"]
fn op_bound_rejects_rehashed_signed_plan_substitutions_and_preserves_probe_budgets() {
    let v = original();
    let r = v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["input"]["rule"] == "isthmus" && r["input"]["chain"] == 10)
        .unwrap();
    let source = &bank::corpus()["cases"][0];
    let c = make(source, r, 2, false);
    assert!(run(&c, &bank::budget()).unwrap().matches_plan);
    for mode in 0..14 {
        let mut c = make(source, r, 2, false);
        let mut b = bank::budget();
        match mode {
            0 => c.fixture.execution_environment.fork = "shanghai".into(),
            1 => c.fixture.state_anchor.timestamp -= 1,
            2 => c.op.attributes_calldata[0] ^= 1,
            3 => b.max_total_input_bytes = 1,
            4 => b.max_reads = 1,
            5 => b.max_total_gas = 1,
            6 => {
                c.fixture
                    .accounts
                    .iter_mut()
                    .find(|a| a.address == c.bound.executor.target)
                    .unwrap()
                    .code = "0x00".into();
            }
            7 => b.max_input_bytes = (c.bound.transaction.data.len() - 2) / 2,
            8 => {
                b.max_total_input_bytes = (c.wire.len() + c.op.attributes_calldata.len() - 1) as u64
            }
            9 => c
                .fixture
                .accounts
                .retain(|a| a.address != "0x420000000000000000000000000000000000001a"),
            10 => {
                c.fixture
                    .accounts
                    .iter_mut()
                    .find(|a| a.address == "0x4200000000000000000000000000000000000015")
                    .unwrap()
                    .storage[0]
                    .value = slot(U256::ZERO)
            }
            11 => {
                c.fixture
                    .accounts
                    .iter_mut()
                    .find(|a| a.address == c.bound.transaction.from)
                    .unwrap()
                    .nonce += 1
            }
            12 => b.max_instructions = 1,
            _ => {
                let mut f = signing::fields(2);
                f[1] = signing::number(U256::from(11));
                let wire = signing::signed(2, &f);
                c.op.transaction =
                    authenticate_user(10, keccak256(&wire), signing::payer(), &wire, wire.len())
                        .unwrap();
            }
        }
        assert!(run(&c, &b).is_err(), "mode {mode}");
    }
    assert_eq!(c.op.transaction.environment().chain_id, Some(10));
    assert_ne!(c.op.transaction.transaction_hash(), B256::ZERO);
}

#[test]
#[ignore = "requires historical OP attributes and complete original local Router bank"]
fn op_bound_exact_maximum_funding_probes_failures_and_control_are_accounted_once() {
    use std::{
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    };
    let v = original();
    let source = &bank::corpus()["cases"][0];
    for rule in [
        "regolith", "canyon", "ecotone", "fjord", "granite", "holocene", "isthmus",
    ] {
        let r = v["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["input"]["rule"] == rule && r["input"]["chain"] == 10)
            .unwrap();
        for exact in [false, true] {
            let mut c = make(source, r, 2, exact);
            let rich = run(&c, &bank::budget()).unwrap();
            let maximum = rich.op_fees.as_ref().unwrap()["chain_cost"]["maximum_funding"]
                .as_str()
                .unwrap()
                .to_owned();
            let payer = c
                .fixture
                .accounts
                .iter_mut()
                .find(|a| a.address == c.bound.transaction.from)
                .unwrap();
            payer.balance = maximum.parse().unwrap();
            let o = run(&c, &bank::budget()).unwrap();
            assert!(o.matches_plan);
            assert_eq!(o.execution.gas_used, rich.execution.gas_used);
            assert_eq!(
                o.op_fees.as_ref().unwrap()["chain_cost"],
                rich.op_fees.as_ref().unwrap()["chain_cost"]
            );
            let n = U256::from_str(&maximum).unwrap() - U256::from(1);
            c.fixture
                .accounts
                .iter_mut()
                .find(|a| a.address == c.bound.transaction.from)
                .unwrap()
                .balance = n.to_string().parse().unwrap();
            assert!(
                run(&c, &bank::budget()).is_err(),
                "{rule} insufficient funds"
            );
        }
    }
    let r = v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["input"]["rule"] == "isthmus" && r["input"]["chain"] == 10)
        .unwrap();
    let mut c = make(source, r, 2, false);
    // The actual ERC20 allowance is zeroed; preflight identity still succeeds,
    // then the original Router reverts with no input/output movement.
    let owner = Address::from_str(&c.bound.transaction.from).unwrap();
    let router = Address::from_str(&c.bound.executor.target).unwrap();
    let first = keccak256(
        [
            U256::from_be_slice(owner.as_slice())
                .to_be_bytes::<32>()
                .as_slice(),
            U256::from(2).to_be_bytes::<32>().as_slice(),
        ]
        .concat(),
    );
    let allowance = keccak256(
        [
            U256::from_be_slice(router.as_slice())
                .to_be_bytes::<32>()
                .as_slice(),
            first.as_slice(),
        ]
        .concat(),
    );
    let token = c
        .fixture
        .accounts
        .iter_mut()
        .find(|a| a.address == c.bound.plan.context.sell_token)
        .unwrap();
    let entry = token
        .storage
        .iter_mut()
        .find(|s| s.slot == format!("{allowance:#x}"))
        .unwrap();
    entry.value = slot(U256::ZERO);
    let failed = run(&c, &bank::budget()).unwrap();
    assert_eq!(failed.execution.status, EvmStatus::Revert);
    assert!(!failed.matches_plan);
    assert_eq!(failed.actual_input_consumed.to_string(), "0");
    assert_eq!(failed.actual_buy_amount.to_string(), "0");
    assert!(
        U256::from_str(
            failed.op_fees.as_ref().unwrap()["chain_cost"]["gross_charge"]
                .as_str()
                .unwrap()
        )
        .unwrap()
            > U256::ZERO
    );
    assert_eq!(
        failed.execution.sender_nonce_after,
        failed.execution.sender_nonce_before + 1
    );
    let c = make(source, r, 2, false);
    let control = core::ExecutionControl {
        deadline: Some(Instant::now()),
        ..Default::default()
    };
    let allowed = vec![c.bound.executor.target.clone()];
    assert_eq!(
        simulate_bound_op_user(
            &c.bound,
            &c.pools,
            &allowed,
            &c.fixture,
            &c.op,
            &bank::budget(),
            control
        )
        .unwrap_err()
        .code,
        SimulationErrorCode::SimulationDeadline
    );
    let progress = Arc::new(AtomicU64::new(0));
    let control = core::ExecutionControl {
        progress: Some(progress.clone()),
        ..Default::default()
    };
    let stop = control.cancellation.clone();
    let handle = std::thread::spawn(move || {
        simulate_bound_op_user(
            &c.bound,
            &c.pools,
            &allowed,
            &c.fixture,
            &c.op,
            &bank::budget(),
            control,
        )
    });
    let start = Instant::now();
    while progress.load(Ordering::Acquire) < 50
        && !handle.is_finished()
        && start.elapsed() < Duration::from_secs(5)
    {
        std::thread::yield_now();
    }
    assert!(progress.load(Ordering::Acquire) >= 50);
    stop.store(true, Ordering::Release);
    assert_eq!(
        handle.join().unwrap().unwrap_err().code,
        SimulationErrorCode::SimulationCancelled
    );
}

fn signed_worker_request(c: &Case) -> xone_simulation::worker::WorkerRequest {
    use xone_simulation::worker::*;
    let e = c.op.transaction.environment();
    WorkerRequest {
        direct_wrap: None,
        protocol: WORKER_PROTOCOL.into(),
        split: None,
        bound: if c.exact.is_some() {
            None
        } else {
            Some(c.bound.clone())
        },
        exact_out_bound: c.exact.clone(),
        prepared: None,
        op_user: Some(WorkerOpUser {
            transaction_hash: format!("{:#x}", c.op.transaction.transaction_hash()),
            signed_wire: format!("0x{}", hex::encode(&c.wire)),
            attributes_calldata: format!("0x{}", hex::encode(&c.op.attributes_calldata)),
        }),
        pools: c.pools.clone(),
        allowed_targets: vec![c.bound.executor.target.clone()],
        fixture: c.fixture.clone(),
        transaction_context: TransactionContext {
            nonce: e.nonce.unwrap(),
            gas_limit: e.gas_limit,
            gas_price: e.gas_price.to_string().parse().unwrap(),
            max_priority_fee_per_gas: e.gas_priority_fee.map(|n| n.to_string().parse().unwrap()),
            chain_id: e.chain_id.unwrap(),
        },
        budget: bank::budget(),
        expected_genesis_hash: c.fixture.state_anchor.genesis_hash.clone(),
        timeout_ms: 60_000,
    }
}
fn worker_binary() -> (tempfile::TempDir, std::path::PathBuf, String) {
    use xone_simulation::worker::executable_sha256;
    let source = std::path::PathBuf::from(
        std::env::var("XONE_OP_ROUTER_BINARY").expect("actual built dex-router binary required"),
    );
    let digest = executable_sha256(&source).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dex-router");
    std::fs::copy(&source, &path).unwrap();
    assert_eq!(executable_sha256(&path).unwrap(), digest);
    assert_eq!(executable_sha256(&source).unwrap(), digest);
    (dir, path, digest)
}
fn invoke_worker(binary: &std::path::Path, input: &Value) -> std::process::Output {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut child = Command::new(binary)
        .arg("--evm-simulation-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(input).unwrap())
        .unwrap();
    child.wait_with_output().unwrap()
}
#[test]
#[ignore = "requires actual built router binary and historical OP attributes"]
fn actual_signed_op_worker_executes_both_bound_modes_and_preserves_the_complete_receipt() {
    use xone_simulation::worker::*;
    let (_dir, binary, digest) = worker_binary();
    let mut index = 0;
    let mut n = 0;
    matrix(|c| {
        let selected = index % 64 < 8;
        index += 1;
        if !selected {
            return;
        }
        let request = signed_worker_request(&c);
        let direct = run(&c, &bank::budget()).unwrap();
        let output = invoke_worker(&binary, &serde_json::to_value(request).unwrap());
        assert!(
            output.status.success(),
            "{} {}",
            c.id,
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.len() <= MAX_WORKER_OUTPUT_BYTES);
        let response: WorkerResponse = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response.protocol, WORKER_PROTOCOL);
        assert_eq!(response.worker_binary_sha256, digest);
        assert!(response.error.is_none());
        assert!(response.prepared.is_none());
        assert_eq!(
            serde_json::to_value(response.outcome.unwrap()).unwrap(),
            serde_json::to_value(direct).unwrap(),
            "{} worker receipt",
            c.id
        );
        n += 1;
    });
    assert_eq!(n, 56);
    assert_eq!(executable_sha256(&binary).unwrap(), digest);
}
#[test]
#[ignore = "requires actual built router binary; malformed signed requests must never execute"]
fn actual_signed_op_worker_rejects_envelope_context_state_and_mode_substitutions() {
    use xone_simulation::worker::*;
    let (_dir, binary, digest) = worker_binary();
    let v = original();
    let r = v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["input"]["rule"] == "isthmus" && r["input"]["chain"] == 10)
        .unwrap();
    let c = make(&bank::corpus()["cases"][0], r, 2, false);
    let source = serde_json::to_value(signed_worker_request(&c)).unwrap();
    for mode in 0..17 {
        let mut q = source.clone();
        match mode {
            0 => q["transaction_context"]["nonce"] = 0.into(),
            1 => q["transaction_context"]["gas_limit"] = 21_000.into(),
            2 => q["transaction_context"]["gas_price"] = "3000000001".into(),
            3 => q["transaction_context"]["max_priority_fee_per_gas"] = "1".into(),
            4 => q["op_user"]["transaction_hash"] = format!("0x{}", "00".repeat(32)).into(),
            5 => q["op_user"]["signed_wire"] = "0x02".into(),
            6 => q["op_user"]["signed_wire"] = format!("0x{}00", hex::encode(&c.wire)).into(),
            7 => q["op_user"]["attributes_calldata"] = "0x00".into(),
            8 => q["fixture"]["execution_environment"]["fork"] = "shanghai".into(),
            9 => q["expected_genesis_hash"] = "0x00".into(),
            10 => q["op_user"]["ignored_field"] = true.into(),
            11 => {
                q.as_object_mut().unwrap().remove("op_user");
            }
            12 => {
                q["exact_out_bound"] = serde_json::to_value(bank::output_bound(
                    &c.bound,
                    &c.pools,
                    c.bound.plan.allocation.gross_buy_amount.clone(),
                    c.bound.plan.allocation.sell_amount.clone(),
                ))
                .unwrap()
            }
            13 => q["budget"]["max_input_bytes"] = 1.into(),
            14 => q["op_user"]["attributes_calldata"] = format!("0x{}", "00".repeat(261)).into(),
            15 => q["transaction_context"]["chain_id"] = 8453.into(),
            _ => q["bound"]["transaction"]["from"] = format!("0x{}", "11".repeat(20)).into(),
        }
        let output = invoke_worker(&binary, &q);
        if output.status.success() {
            let response: WorkerResponse = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(response.worker_binary_sha256, digest);
            assert!(
                response.error.is_some()
                    && response.outcome.is_none()
                    && response.prepared.is_none(),
                "mode {mode}"
            );
        } else {
            assert!(
                output.stdout.is_empty(),
                "rejected input emitted stdout: {mode}"
            );
            assert!(!output.stderr.is_empty());
        }
    }
    let mut old = source;
    old.as_object_mut().unwrap().remove("op_user");
    let parsed: WorkerRequest = serde_json::from_value(old).unwrap();
    assert!(parsed.op_user.is_none());
    assert!(!serde_json::to_value(parsed)
        .unwrap()
        .as_object()
        .unwrap()
        .contains_key("op_user"));
}

#[test]
#[ignore = "requires retained historical OP attributes"]
fn retained_op_worker_receipt_commitment_audits_modes_and_reserves_parent_reads() {
    use xone_simulation::worker_op::OpReceiptCommitment;
    let mut index = 0;
    let mut n = 0;
    matrix(|c| {
        let selected = index % 64 < 4;
        index += 1;
        if !selected {
            return;
        }
        let request = signed_worker_request(&c);
        let outcome = run(&c, &request.budget).unwrap();
        let image = c.fixture.to_image(&request.budget).unwrap();
        let commitment = OpReceiptCommitment::from_request(&request).unwrap();
        let audit = commitment
            .audit(image.clone(), &outcome, Default::default())
            .unwrap();
        assert_eq!(
            audit.total_usage.reads,
            outcome.usage.reads + audit.fees.telemetry.reads.reads
        );
        assert_eq!(
            audit.total_usage.read_bytes,
            outcome.usage.read_bytes + audit.fees.telemetry.reads.bytes
        );
        assert_eq!(
            serde_json::to_value(audit.fees.costs).unwrap(),
            outcome.op_fees.as_ref().unwrap()["chain_cost"]
        );
        assert!(audit.total_usage.reads <= request.budget.max_reads);
        for mode in 0..9 {
            let mut changed = serde_json::to_value(&outcome).unwrap();
            match mode {
                0 => changed["op_fees"]["chain_cost"]["execution_fee"] = "0".into(),
                1 => {
                    let accounts = changed["op_fees"]["accounts"].as_object_mut().unwrap();
                    let a = accounts.values_mut().next().unwrap();
                    for f in ["before", "after"] {
                        a[f] = (U256::from_str(a[f].as_str().unwrap()).unwrap() + U256::from(1))
                            .to_string()
                            .into();
                    }
                }
                2 => changed["source_fixture_hash"] = "changed".into(),
                3 => changed["execution"]["sender_nonce_after"] = 0.into(),
                4 => changed["usage"]["reads"] = request.budget.max_reads.into(),
                5 => changed["usage"]["read_bytes"] = request.budget.max_read_bytes.into(),
                6 => changed["op_fees"] = Value::Null,
                7 => {
                    changed["usage"]["instructions"] = (request.budget.max_instructions + 1).into()
                }
                _ => changed["simulator_version"] = "provided-local-state/shanghai-v1".into(),
            }
            changed.as_object_mut().unwrap().remove("receipt_hash");
            let bound = request
                .bound
                .as_ref()
                .map(|b| serde_json::to_value(b).unwrap())
                .unwrap_or_else(|| {
                    serde_json::to_value(request.exact_out_bound.as_ref().unwrap()).unwrap()
                });
            let h = xone_domain::hash(
                &json!({"bound":bound,"source_fixture_hash":changed["source_fixture_hash"],
                "transaction_context":request.transaction_context,"limits":request.budget,
                "simulator_version":changed["simulator_version"],"outcome":changed}),
            )
            .unwrap();
            changed["receipt_hash"] = h.into();
            let changed: SimulationOutcome = serde_json::from_value(changed).unwrap();
            assert!(
                commitment
                    .audit(image.clone(), &changed, Default::default())
                    .is_err(),
                "{} mode {mode}",
                c.id
            );
        }
        n += 1;
    });
    assert_eq!(n, 28);
}

#[test]
#[ignore = "exports retained sources for normal OP HTTP build and wallet signing"]
fn export_original_op_bound_inputs_for_normal_op_api_sources() {
    let mut rows = Vec::new();
    let mut index = 0;
    matrix(|c| {
        let selected = index % 64 < 4;
        index += 1;
        if selected {
            rows.push(json!({"id":c.id,"request":signed_worker_request(&c)}));
        }
    });
    assert_eq!(rows.len(), 28);
    std::fs::write(
        std::env::var("XONE_OP_API_INPUTS").unwrap(),
        serde_json::to_vec_pretty(&rows).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "exports historical Jovian sources for normal HTTP build/sign/simulate"]
fn export_original_op_bound_inputs_for_jovian_normal_api_sources() {
    let v: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_OP_JOVIAN_VECTORS").unwrap()).unwrap(),
    )
    .unwrap();
    let r = &v["cases"][0];
    assert_eq!(r["input"]["rule"], "jovian");
    let corpus = bank::corpus();
    let c = corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["service_eligible"] == true)
        .unwrap();
    let mut rows = Vec::new();
    for kind in [0, 2] {
        for exact in [false, true] {
            let c = make(c, r, kind, exact);
            rows.push(json!({"id":c.id,"request":signed_worker_request(&c)}));
        }
    }
    assert_eq!(rows.len(), 4);
    std::fs::write(
        std::env::var("XONE_OP_JOVIAN_API_INPUTS").unwrap(),
        serde_json::to_vec_pretty(&rows).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "exports scoped Karst sources for normal HTTP signing"]
fn export_original_op_bound_inputs_for_karst_normal_api_sources() {
    let v: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_OP_JOVIAN_VECTORS").unwrap()).unwrap(),
    )
    .unwrap();
    let mut r = v["cases"][0].clone();
    r["input"]["rule"] = "karst".into();
    r["input"]["timestamp"] = 1_783_526_403u64.into();
    let corpus = bank::corpus();
    let c = corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["service_eligible"] == true)
        .unwrap();
    let mut rows = Vec::new();
    for kind in [0, 2] {
        for exact in [false, true] {
            let c = make(c, &r, kind, exact);
            rows.push(json!({"id":c.id,"request":signed_worker_request(&c)}));
        }
    }
    assert_eq!(rows.len(), 4);
    std::fs::write(
        std::env::var("XONE_OP_KARST_API_INPUTS").unwrap(),
        serde_json::to_vec_pretty(&rows).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "requires complete original Karst Router transitions"]
fn karst_bound_router_tokens_fees_and_complete_poststate_match_full_original() {
    let v: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_OP_JOVIAN_VECTORS").unwrap()).unwrap(),
    )
    .unwrap();
    let mut r = v["cases"][0].clone();
    r["input"]["rule"] = "karst".into();
    r["input"]["timestamp"] = 1_783_526_403u64.into();
    let original: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_OP_KARST_BOUND_VECTORS").unwrap()).unwrap(),
    )
    .unwrap();
    let rows = original["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(original["passed"], true);
    let corpus = bank::corpus();
    let c = corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["service_eligible"] == true)
        .unwrap();
    let mut count = 0;
    for kind in [0, 2] {
        for exact in [false, true] {
            let c = make(c, &r, kind, exact);
            let expected = rows.iter().find(|r| r["id"] == c.id).unwrap();
            let out = run(&c, &bank::budget()).unwrap();
            complete_state(&c, &out, expected);
            assert!(out.matches_plan);
            assert!(!out.verified);
            assert_eq!(
                out.actual_input_consumed.to_string(),
                expected["spent"].as_str().unwrap()
            );
            assert_eq!(
                out.actual_buy_amount.to_string(),
                expected["bought"].as_str().unwrap()
            );
            assert_eq!(
                out.execution.gas_used,
                expected["gas_used"].as_u64().unwrap()
            );
            assert_eq!(
                out.op_fees.as_ref().unwrap()["chain_cost"]["l1_data_fee"],
                expected["l1_fee"]
            );
            assert_eq!(
                out.op_fees.as_ref().unwrap()["chain_cost"]["operator_fee"],
                expected["operator_fee"]
            );
            count += 1;
        }
    }
    assert_eq!(count, 4);
}
