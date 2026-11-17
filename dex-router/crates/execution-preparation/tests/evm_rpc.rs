#[path = "support/evm_rpc_fixture.rs"]
mod source;
use source::{request, serve};
use std::collections::BTreeMap;
use xone_domain::hash;
use xone_execution_plan::{bind_v2_execution, build};
use xone_execution_preparation::evm_rpc::{self, Bound};
#[tokio::test]
async fn fixed_original_state_is_checked_and_rpc_commitment_is_bound() {
    let (b, p, c, requests, task) = serve("ready").await;
    let result = evm_rpc::prepare(&c, Bound::ExactIn(&b), &p, request()).await;
    task.abort();
    let result = result.unwrap();
    assert_eq!(result.result["component"]["ready"], true);
    assert_eq!(result.result["component"]["signed"], false);
    assert_eq!(result.result["component"]["submitted"], false);
    assert_eq!(result.result["rpc_observation"]["consensusVerified"], false);
    assert_eq!(
        result.result["rpc_observation"]["captureHash"],
        hash(&result.observations).unwrap()
    );
    let mut committed = result.result.clone();
    let expected = committed
        .as_object_mut()
        .unwrap()
        .remove("rpc_preparation_hash")
        .unwrap();
    assert_eq!(expected, hash(&committed).unwrap());
    let component = result.result["component"].clone();
    let _: xone_execution_preparation::evm::Preparation =
        serde_json::from_value(component.clone()).unwrap();
    let mut plain = component.clone();
    let expected = plain
        .as_object_mut()
        .unwrap()
        .remove("preparation_hash")
        .unwrap();
    assert_eq!(expected, hash(&plain).unwrap());
    let calls = requests.lock().unwrap();
    assert!(calls.len() > 25);
    assert!(calls.iter().all(|q| !matches!(
        q["method"].as_str(),
        Some("eth_sendRawTransaction" | "eth_sendTransaction" | "personal_sign")
    )));
    let block = &b.plan.snapshot.block.hash;
    for q in calls.iter().filter(|q| {
        matches!(
            q["method"].as_str(),
            Some("eth_call" | "eth_getCode" | "eth_getStorageAt" | "eth_getBalance")
        )
    }) {
        let selector = q["params"].as_array().unwrap().last().unwrap();
        assert_eq!(selector["requireCanonical"], true);
        assert!(selector["blockHash"] == *block || q["id"].as_u64().unwrap() < 9);
    }
}
#[tokio::test]
async fn identity_state_nonce_and_envelope_faults_never_emit_transactions() {
    for (mode, expected) in [
        ("wrong_chain", "EVM_RPC_WRONG_CHAIN"),
        ("wrong_genesis", "EVM_RPC_WRONG_GENESIS"),
        ("ignored_selector", "EVM_RPC_HASH_SELECTOR_IGNORED"),
        ("ignored_code", "EVM_RPC_HASH_SELECTOR_IGNORED"),
        ("ignored_storage", "EVM_RPC_HASH_SELECTOR_IGNORED"),
        ("ignored_call", "EVM_RPC_HASH_SELECTOR_IGNORED"),
        ("ignored_nonce", "EVM_RPC_HASH_SELECTOR_IGNORED"),
        ("caller_contract", "EVM_RPC_CONTRACT_CALLER_UNSUPPORTED"),
        ("code", "EVM_RPC_CODE_CHANGED"),
        ("lock", "QUOTE_STATE_CHANGED"),
        ("reserves", "QUOTE_STATE_CHANGED"),
        ("pending", "EVM_RPC_PENDING_TRANSACTIONS"),
        ("nonce_changed", "PREPARATION_EVM_NONCE_CHANGED"),
        ("reorg", "EVM_RPC_BLOCK_CHANGED"),
        ("id", "EVM_RPC_ENVELOPE"),
    ] {
        let (b, p, c, _, task) = serve(mode).await;
        let result = evm_rpc::prepare(&c, Bound::ExactIn(&b), &p, request()).await;
        task.abort();
        assert_eq!(result.unwrap_err().0, expected, "{mode}");
    }
}
#[tokio::test]
async fn insufficient_funding_is_captured_without_unsigned_wire() {
    for mode in ["funding", "allowance"] {
        let (b, p, c, _, task) = serve(mode).await;
        let result = evm_rpc::prepare(&c, Bound::ExactIn(&b), &p, request()).await;
        task.abort();
        let r = result.unwrap();
        assert_eq!(r.result["component"]["ready"], false);
        assert!(r.result["component"]["unsigned_transaction"].is_null());
        assert!(r.result["component"]["rpc_transaction"].is_null());
        assert!(r.result["component"]["blockers"].as_array().unwrap().len() == 1);
    }
}
#[tokio::test]
async fn budgets_and_deadlines_fail_closed() {
    for mode in ["calls", "bytes", "timeout", "expired", "token_pin"] {
        let (b, p, mut c, _, task) = serve(if mode == "timeout" {
            "timeout"
        } else {
            "ready"
        })
        .await;
        match mode {
            "calls" => c.max_calls = 32,
            "bytes" => c.max_response_bytes = 65536,
            "timeout" => c.timeout_ms = 100,
            "token_pin" => c.token_code_sha256 = BTreeMap::new(),
            _ => {}
        }
        let mut bound = b;
        if mode == "expired" {
            bound.plan.context.deadline_ms = bound.plan.snapshot.block.timestamp_ms + 1;
            let changed = build(
                &bound.plan.context,
                &bound.plan.allocation,
                &bound.plan.snapshot,
                &bound.plan.economic_policy_hash,
            )
            .unwrap();
            bound = bind_v2_execution(
                &changed,
                &p,
                &c.executor,
                std::slice::from_ref(&c.executor.target),
            )
            .unwrap();
        }
        let r = evm_rpc::prepare(&c, Bound::ExactIn(&bound), &p, request()).await;
        task.abort();
        let error = r.unwrap_err().0;
        let expected = match mode {
            "calls" => "EVM_RPC_CALL_BUDGET",
            "bytes" => "EVM_RPC_RESPONSE_BUDGET",
            "timeout" => "EVM_RPC_TIMEOUT",
            "expired" => "QUOTE_EXPIRED",
            _ => "EVM_RPC_TOKEN_NOT_ADMITTED",
        };
        assert_eq!(error, expected, "{mode}");
    }
}
