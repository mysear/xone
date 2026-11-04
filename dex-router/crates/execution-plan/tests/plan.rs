use xone_domain::*;
use xone_execution_plan::*;
use xone_optimizer::*;
use xone_routing::*;
#[test]
fn economic_hash_includes_recipient_and_slippage() {
    let s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    let amount = "10000".parse().unwrap();
    let paths = search(
        &Graph::build(&s.pools).unwrap(),
        &s.pools,
        "A",
        "B",
        &amount,
        Budget {
            split_parts: 20,
            max_hops: 1,
            max_candidates: 4,
            max_expansions: 100,
        },
    )
    .unwrap();
    let allocation =
        optimize(&s.pools, &paths.candidates, &amount, 20, 1, &Amount::zero()).unwrap();
    let context = Context {
        chain_key: "evm:31337".into(),
        caller: "caller".into(),
        recipient: "alice".into(),
        sell_token: "A".into(),
        buy_token: "B".into(),
        slippage_bps: 50,
        deadline_ms: 2000,
    };
    let a = build(&context, &allocation, &s.reference, "policy").unwrap();
    let mut changed = context.clone();
    changed.recipient = "bob".into();
    let b = build(&changed, &allocation, &s.reference, "policy").unwrap();
    assert_ne!(a.plan_hash, b.plan_hash);
    assert!(evm_transaction(&a, "0x0000000000000000000000000000000000000001", &[]).is_err());
}

#[test]
fn v3_quote_cannot_be_encoded_as_v2_router_calldata() {
    let p: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/v3-state.json")).unwrap();
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    snapshot.pools = vec![p.clone()];
    snapshot.reference.scope_hash = scope_hash(&snapshot.pools).unwrap();
    let amount = "10000".parse().unwrap();
    let paths = search(
        &Graph::build(&snapshot.pools).unwrap(),
        &snapshot.pools,
        &p.token0,
        &p.token1,
        &amount,
        Budget {
            split_parts: 20,
            max_hops: 1,
            max_candidates: 4,
            max_expansions: 10,
        },
    )
    .unwrap();
    let allocation = optimize(
        &snapshot.pools,
        &paths.candidates,
        &amount,
        20,
        1,
        &Amount::zero(),
    )
    .unwrap();
    let context = Context {
        chain_key: p.chain_key,
        caller: p.token0.clone(),
        recipient: p.token1.clone(),
        sell_token: p.token0,
        buy_token: p.token1,
        slippage_bps: 50,
        deadline_ms: 2000,
    };
    let plan = build(&context, &allocation, &snapshot.reference, "policy").unwrap();
    let target = "0x0000000000000000000000000000000000000001";
    assert_eq!(
        evm_transaction(&plan, target, &[target.into()])
            .unwrap_err()
            .0,
        "EXECUTION_ADAPTER_UNSUPPORTED"
    );
}

#[test]
fn singleton_quotes_cannot_be_encoded_as_v2_router_calldata() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/singleton-state.json")).unwrap();
    let pools: Vec<Pool> = serde_json::from_value(f["pools"].clone()).unwrap();
    for p in pools {
        let mut snapshot: Snapshot =
            serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
        snapshot.pools = vec![p.clone()];
        snapshot.reference.scope_hash = scope_hash(&snapshot.pools).unwrap();
        let amount = "10000".parse().unwrap();
        let paths = search(
            &Graph::build(&snapshot.pools).unwrap(),
            &snapshot.pools,
            &p.token0,
            &p.token1,
            &amount,
            Budget {
                split_parts: 20,
                max_hops: 1,
                max_candidates: 4,
                max_expansions: 10,
            },
        )
        .unwrap();
        let allocation = optimize(
            &snapshot.pools,
            &paths.candidates,
            &amount,
            20,
            1,
            &Amount::zero(),
        )
        .unwrap();
        let context = Context {
            chain_key: p.chain_key,
            caller: p.token0.clone(),
            recipient: p.token1.clone(),
            sell_token: p.token0,
            buy_token: p.token1,
            slippage_bps: 50,
            deadline_ms: 2000,
        };
        let plan = build(&context, &allocation, &snapshot.reference, "policy").unwrap();
        let target = "0x0000000000000000000000000000000000000001";
        assert_eq!(
            evm_transaction(&plan, target, &[target.into()])
                .unwrap_err()
                .0,
            "EXECUTION_ADAPTER_UNSUPPORTED"
        );
    }
}

#[test]
fn raydium_quote_cannot_be_encoded_as_evm_v2_calldata() {
    let p: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/raydium-state.json")).unwrap();
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    snapshot.pools = vec![p.clone()];
    snapshot.reference.block.chain_key = p.chain_key.clone();
    snapshot.reference.scope_hash = scope_hash(&snapshot.pools).unwrap();
    let amount = "10000".parse().unwrap();
    let paths = search(
        &Graph::build(&snapshot.pools).unwrap(),
        &snapshot.pools,
        &p.token0,
        &p.token1,
        &amount,
        Budget {
            split_parts: 20,
            max_hops: 1,
            max_candidates: 4,
            max_expansions: 10,
        },
    )
    .unwrap();
    let allocation = optimize(
        &snapshot.pools,
        &paths.candidates,
        &amount,
        20,
        1,
        &Amount::zero(),
    )
    .unwrap();
    let context = Context {
        chain_key: p.chain_key,
        caller: p.token0.clone(),
        recipient: p.token1.clone(),
        sell_token: p.token0,
        buy_token: p.token1,
        slippage_bps: 50,
        deadline_ms: 2000,
    };
    let plan = build(&context, &allocation, &snapshot.reference, "policy").unwrap();
    let target = "0x0000000000000000000000000000000000000001";
    assert!(evm_transaction(&plan, target, &[target.into()]).is_err());
}

#[test]
fn orca_quote_cannot_be_encoded_as_evm_v2_calldata() {
    let p: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/orca-state.json")).unwrap();
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    snapshot.pools = vec![p.clone()];
    snapshot.reference.block.chain_key = p.chain_key.clone();
    snapshot.reference.scope_hash = scope_hash(&snapshot.pools).unwrap();
    let amount = "10000".parse().unwrap();
    let paths = search(
        &Graph::build(&snapshot.pools).unwrap(),
        &snapshot.pools,
        &p.token0,
        &p.token1,
        &amount,
        Budget {
            split_parts: 20,
            max_hops: 1,
            max_candidates: 4,
            max_expansions: 10,
        },
    )
    .unwrap();
    let allocation = optimize(
        &snapshot.pools,
        &paths.candidates,
        &amount,
        20,
        1,
        &Amount::zero(),
    )
    .unwrap();
    let context = Context {
        chain_key: p.chain_key,
        caller: p.token0.clone(),
        recipient: p.token1.clone(),
        sell_token: p.token0,
        buy_token: p.token1,
        slippage_bps: 50,
        deadline_ms: 2000,
    };
    let plan = build(&context, &allocation, &snapshot.reference, "policy").unwrap();
    let target = "0x0000000000000000000000000000000000000001";
    assert!(evm_transaction(&plan, target, &[target.into()]).is_err());
}

#[test]
fn rayclmm_quote_cannot_be_encoded_as_evm_v2_calldata() {
    let p: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/rayclmm-state.json")).unwrap();
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    snapshot.pools = vec![p.clone()];
    snapshot.reference.block.chain_key = p.chain_key.clone();
    snapshot.reference.scope_hash = scope_hash(&snapshot.pools).unwrap();
    let amount = "10000".parse().unwrap();
    let paths = search(
        &Graph::build(&snapshot.pools).unwrap(),
        &snapshot.pools,
        &p.token0,
        &p.token1,
        &amount,
        Budget {
            split_parts: 20,
            max_hops: 1,
            max_candidates: 4,
            max_expansions: 10,
        },
    )
    .unwrap();
    let allocation = optimize(
        &snapshot.pools,
        &paths.candidates,
        &amount,
        20,
        1,
        &Amount::zero(),
    )
    .unwrap();
    let context = Context {
        chain_key: p.chain_key,
        caller: p.token0.clone(),
        recipient: p.token1.clone(),
        sell_token: p.token0,
        buy_token: p.token1,
        slippage_bps: 50,
        deadline_ms: 2000,
    };
    let plan = build(&context, &allocation, &snapshot.reference, "policy").unwrap();
    let target = "0x0000000000000000000000000000000000000001";
    assert!(evm_transaction(&plan, target, &[target.into()]).is_err());
}

#[test]
fn rayclmm_token_fee_quote_cannot_be_encoded_as_evm_v2_calldata() {
    let mut p: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/rayclmm-state.json")).unwrap();
    p.protocol = "raydium-clmm-token-fee".into();
    p.adapter_version = "raydium-clmm/6d7a0f82-spl-token-fee-1".into();
    p.raydium_clmm.as_mut().unwrap().collect_fee_on = 1;
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    snapshot.pools = vec![p.clone()];
    snapshot.reference.block.chain_key = p.chain_key.clone();
    snapshot.reference.scope_hash = scope_hash(&snapshot.pools).unwrap();
    let amount = "10000".parse().unwrap();
    let paths = search(
        &Graph::build(&snapshot.pools).unwrap(),
        &snapshot.pools,
        &p.token0,
        &p.token1,
        &amount,
        Budget {
            split_parts: 20,
            max_hops: 1,
            max_candidates: 4,
            max_expansions: 10,
        },
    )
    .unwrap();
    let allocation = optimize(
        &snapshot.pools,
        &paths.candidates,
        &amount,
        20,
        1,
        &Amount::zero(),
    )
    .unwrap();
    let context = Context {
        chain_key: p.chain_key,
        caller: p.token0.clone(),
        recipient: p.token1.clone(),
        sell_token: p.token0,
        buy_token: p.token1,
        slippage_bps: 50,
        deadline_ms: 2000,
    };
    let plan = build(&context, &allocation, &snapshot.reference, "policy").unwrap();
    let target = "0x0000000000000000000000000000000000000001";
    assert!(evm_transaction(&plan, target, &[target.into()]).is_err());
}

#[test]
fn rayclmm_bitmap_quote_cannot_be_encoded_as_evm_v2_calldata() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/rayclmm-bitmap-quotes.json"
    ))
    .unwrap();
    let p: Pool = serde_json::from_value(fixture["base_pool"].clone()).unwrap();
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    snapshot.pools = vec![p.clone()];
    snapshot.reference.block.chain_key = p.chain_key.clone();
    snapshot.reference.scope_hash = scope_hash(&snapshot.pools).unwrap();
    let amount = "10000".parse().unwrap();
    let paths = search(
        &Graph::build(&snapshot.pools).unwrap(),
        &snapshot.pools,
        &p.token0,
        &p.token1,
        &amount,
        Budget {
            split_parts: 20,
            max_hops: 1,
            max_candidates: 4,
            max_expansions: 10,
        },
    )
    .unwrap();
    let allocation = optimize(
        &snapshot.pools,
        &paths.candidates,
        &amount,
        20,
        1,
        &Amount::zero(),
    )
    .unwrap();
    let context = Context {
        chain_key: p.chain_key,
        caller: p.token0.clone(),
        recipient: p.token1.clone(),
        sell_token: p.token0,
        buy_token: p.token1,
        slippage_bps: 50,
        deadline_ms: 2000,
    };
    let plan = build(&context, &allocation, &snapshot.reference, "policy").unwrap();
    let target = "0x0000000000000000000000000000000000000001";
    assert!(evm_transaction(&plan, target, &[target.into()]).is_err());
}

#[test]
fn rayclmm_seed_quote_cannot_be_encoded_as_evm_v2_calldata() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/rayclmm-seed-quotes.json"
    ))
    .unwrap();
    let p: Pool = serde_json::from_value(fixture["base_pool"].clone()).unwrap();
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    snapshot.pools = vec![p.clone()];
    snapshot.reference.block.chain_key = p.chain_key.clone();
    snapshot.reference.scope_hash = scope_hash(&snapshot.pools).unwrap();
    let amount = "10000".parse().unwrap();
    let paths = search(
        &Graph::build(&snapshot.pools).unwrap(),
        &snapshot.pools,
        &p.token0,
        &p.token1,
        &amount,
        Budget {
            split_parts: 20,
            max_hops: 1,
            max_candidates: 4,
            max_expansions: 10,
        },
    )
    .unwrap();
    let allocation = optimize(
        &snapshot.pools,
        &paths.candidates,
        &amount,
        20,
        1,
        &Amount::zero(),
    )
    .unwrap();
    let context = Context {
        chain_key: p.chain_key,
        caller: p.token0.clone(),
        recipient: p.token1.clone(),
        sell_token: p.token0,
        buy_token: p.token1,
        slippage_bps: 50,
        deadline_ms: 2000,
    };
    let plan = build(&context, &allocation, &snapshot.reference, "policy").unwrap();
    let target = "0x0000000000000000000000000000000000000001";
    assert!(evm_transaction(&plan, target, &[target.into()]).is_err());
}

#[test]
fn rayclmm_dynamic_quote_cannot_be_encoded_as_evm_v2_calldata() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/rayclmm-dynamic-quotes.json"
    ))
    .unwrap();
    let p: Pool = serde_json::from_value(fixture["base_pool"].clone()).unwrap();
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    snapshot.pools = vec![p.clone()];
    snapshot.reference.block.chain_key = p.chain_key.clone();
    snapshot.reference.scope_hash = scope_hash(&snapshot.pools).unwrap();
    let amount = "10000".parse().unwrap();
    let paths = search(
        &Graph::build(&snapshot.pools).unwrap(),
        &snapshot.pools,
        &p.token0,
        &p.token1,
        &amount,
        Budget {
            split_parts: 20,
            max_hops: 1,
            max_candidates: 4,
            max_expansions: 10,
        },
    )
    .unwrap();
    let allocation = optimize(
        &snapshot.pools,
        &paths.candidates,
        &amount,
        20,
        1,
        &Amount::zero(),
    )
    .unwrap();
    let context = Context {
        chain_key: p.chain_key,
        caller: p.token0.clone(),
        recipient: p.token1.clone(),
        sell_token: p.token0,
        buy_token: p.token1,
        slippage_bps: 50,
        deadline_ms: 2000,
    };
    let plan = build(&context, &allocation, &snapshot.reference, "policy").unwrap();
    let target = "0x0000000000000000000000000000000000000001";
    assert!(evm_transaction(&plan, target, &[target.into()]).is_err());
}

#[test]
fn weighted_quote_cannot_be_encoded_as_v2_router_calldata() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/weighted-state.json")).unwrap();
    let p: Pool = serde_json::from_value(fixture["pools"][0].clone()).unwrap();
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    snapshot.pools = vec![p.clone()];
    snapshot.reference.scope_hash = scope_hash(&snapshot.pools).unwrap();
    let amount = "1000000000000000000".parse().unwrap();
    let paths = search(
        &Graph::build(&snapshot.pools).unwrap(),
        &snapshot.pools,
        &p.token0,
        &p.token1,
        &amount,
        Budget {
            split_parts: 20,
            max_hops: 1,
            max_candidates: 4,
            max_expansions: 10,
        },
    )
    .unwrap();
    let allocation = optimize(
        &snapshot.pools,
        &paths.candidates,
        &amount,
        20,
        1,
        &Amount::zero(),
    )
    .unwrap();
    let context = Context {
        chain_key: p.chain_key,
        caller: p.token0.clone(),
        recipient: p.token1.clone(),
        sell_token: p.token0,
        buy_token: p.token1,
        slippage_bps: 50,
        deadline_ms: 2000,
    };
    let plan = build(&context, &allocation, &snapshot.reference, "policy").unwrap();
    let target = "0x0000000000000000000000000000000000000001";
    assert_eq!(
        evm_transaction(&plan, target, &[target.into()])
            .unwrap_err()
            .0,
        "EXECUTION_ADAPTER_UNSUPPORTED"
    );
}

#[test]
fn weighted_static_quote_cannot_be_encoded_as_v2_router_calldata() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/weighted-static-state.json"
    ))
    .unwrap();
    let p: Pool = serde_json::from_value(fixture["pools"][0].clone()).unwrap();
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    snapshot.pools = vec![p.clone()];
    snapshot.reference.scope_hash = scope_hash(&snapshot.pools).unwrap();
    let amount = "1000000000000000000".parse().unwrap();
    let paths = search(
        &Graph::build(&snapshot.pools).unwrap(),
        &snapshot.pools,
        &p.token0,
        &p.token1,
        &amount,
        Budget {
            split_parts: 20,
            max_hops: 1,
            max_candidates: 4,
            max_expansions: 10,
        },
    )
    .unwrap();
    let allocation = optimize(
        &snapshot.pools,
        &paths.candidates,
        &amount,
        20,
        1,
        &Amount::zero(),
    )
    .unwrap();
    let context = Context {
        chain_key: p.chain_key,
        caller: p.token0.clone(),
        recipient: p.token1.clone(),
        sell_token: p.token0,
        buy_token: p.token1,
        slippage_bps: 50,
        deadline_ms: 2000,
    };
    let plan = build(&context, &allocation, &snapshot.reference, "policy").unwrap();
    let target = "0x0000000000000000000000000000000000000001";
    assert_eq!(
        evm_transaction(&plan, target, &[target.into()])
            .unwrap_err()
            .0,
        "EXECUTION_ADAPTER_UNSUPPORTED"
    );
}
