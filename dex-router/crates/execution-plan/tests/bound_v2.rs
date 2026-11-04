use xone_domain::{hash, protocols::V2State, scope_hash, Amount, Pool, Snapshot};
use xone_execution_plan::*;
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{Edge, PathQuote};

const CALLER: &str = "0x0000000000000000000000000000000000000011";
const RECIPIENT: &str = "0x0000000000000000000000000000000000000022";
const TOKEN0: &str = "0x0000000000000000000000000000000000000033";
const TOKEN1: &str = "0x0000000000000000000000000000000000000044";
const PAIR: &str = "0x0000000000000000000000000000000000000055";
const FACTORY: &str = "0x0000000000000000000000000000000000000066";
const ROUTER: &str = "0x0000000000000000000000000000000000000077";

fn fixture(reverse: bool) -> (Plan, Vec<Pool>) {
    // Local in-memory quote state only; this fixture proves no EVM execution.
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    snapshot.pools.truncate(1);
    let pool = &mut snapshot.pools[0];
    pool.pool_id = PAIR.into();
    pool.token0 = TOKEN0.into();
    pool.token1 = TOKEN1.into();
    pool.reserve1 = "2000000".parse().unwrap();
    pool.v2 = Some(V2State {
        swap: None,
        factory: FACTORY.into(),
        code_sha256: "ab".repeat(32),
    });
    snapshot.reference.scope_hash = scope_hash(&snapshot.pools).unwrap();
    snapshot.reference.adapter_set_hash = hash(&vec!["uniswap-v2/1"]).unwrap();
    let (sell, buy) = if reverse {
        (TOKEN1, TOKEN0)
    } else {
        (TOKEN0, TOKEN1)
    };
    let amount: Amount = "10000".parse().unwrap();
    let routes = vec![AllocatedRoute {
        path: PathQuote {
            amount_in: amount.clone(),
            edges: vec![Edge {
                pool_id: PAIR.into(),
                protocol: "uniswap-v2".into(),
                zero_for_one: !reverse,
                from_token: None,
                to: buy.into(),
            }],
            tokens: vec![sell.into(), buy.into()],
            amount_out: Amount::zero(),
        },
        amount_in: amount,
        amount_out: Amount::zero(),
    }];
    let allocation = evaluate_allocation(&snapshot.pools, &routes, &Amount::zero()).unwrap();
    let context = Context {
        chain_key: snapshot.reference.block.chain_key.clone(),
        caller: CALLER.into(),
        recipient: RECIPIENT.into(),
        sell_token: sell.into(),
        buy_token: buy.into(),
        slippage_bps: 50,
        deadline_ms: snapshot.reference.block.timestamp_ms + 60_000,
    };
    (
        build(&context, &allocation, &snapshot.reference, "policy").unwrap(),
        snapshot.pools,
    )
}

#[test]
fn legacy_encoder_rejects_mutated_plan_commitment() {
    let (mut plan, _) = fixture(false);
    plan.context.recipient = CALLER.into();
    assert!(evm_transaction(&plan, ROUTER, &[ROUTER.into()]).is_err());
}

fn executor() -> ExecutorIdentity {
    ExecutorIdentity {
        target: ROUTER.into(),
        code_sha256: "cd".repeat(32),
        factory: FACTORY.into(),
        factory_code_sha256: "ef".repeat(32),
        adapter_version: "uniswap-v2-router02/1.1.0-beta.0".into(),
    }
}

fn bind(plan: &Plan, pools: &[Pool]) -> xone_domain::Result<BoundEvmExecution> {
    bind_v2_execution(plan, pools, &executor(), &[ROUTER.into()])
}

fn recommit(plan: &mut Plan) {
    *plan = build(
        &plan.context,
        &plan.allocation,
        &plan.snapshot,
        &plan.economic_policy_hash,
    )
    .unwrap();
}

fn reseal(bound: &mut BoundEvmExecution) {
    let mut value = serde_json::to_value(&bound).unwrap();
    value.as_object_mut().unwrap().remove("binding_hash");
    bound.binding_hash = hash(&value).unwrap();
}

#[test]
fn plan_hash_verification_preserves_generic_quote_contexts() {
    let (mut plan, _) = fixture(false);
    plan.context.caller = "legacy caller".into();
    plan.context.recipient = "legacy recipient".into();
    recommit(&mut plan);
    verify_plan_hash(&plan).unwrap();
    for field in [
        "context",
        "snapshot",
        "allocation",
        "economic_policy_hash",
        "plan_hash",
        "min_buy_amount",
    ] {
        let mut value = serde_json::to_value(&plan).unwrap();
        match field {
            "context" => value[field]["recipient"] = "changed".into(),
            "snapshot" => value[field]["block"]["number_or_slot"] = 999.into(),
            "allocation" => value[field]["refund"] = "1".into(),
            "economic_policy_hash" | "plan_hash" => value[field] = "changed".into(),
            "min_buy_amount" => value[field] = "1".into(),
            _ => unreachable!(),
        }
        let changed = serde_json::from_value(value).unwrap();
        assert!(verify_plan_hash(&changed).is_err(), "{field}");
    }
    // Rehashing the supplied wrong minOut is insufficient: recompute slippage too.
    plan.min_buy_amount = "1".parse().unwrap();
    plan.plan_hash = hash(&serde_json::json!({
        "context":plan.context, "snapshot":plan.snapshot, "allocation":plan.allocation,
        "min_buy_amount":plan.min_buy_amount, "economic_policy_hash":plan.economic_policy_hash,
    }))
    .unwrap();
    assert!(verify_plan_hash(&plan).is_err());
}

#[test]
fn forward_and_reverse_bindings_preserve_distinct_caller_recipient_and_exact_calldata() {
    for reverse in [false, true] {
        let (mut plan, pools) = fixture(reverse);
        // Economic activation costs need not equal the actual recipient balance delta.
        plan.allocation.net_buy_amount = "1".parse().unwrap();
        recommit(&mut plan);
        let bound = bind(&plan, &pools).unwrap();
        validate_bound_execution(&bound, &pools, &[ROUTER.into()]).unwrap();
        assert_eq!(bound.transaction.from, CALLER);
        assert_eq!(bound.transaction.to, ROUTER);
        assert_eq!(bound.transaction.chain_key, "evm:31337");
        assert_eq!(bound.transaction.value, Amount::zero());
        assert_eq!(bound.touched_pool_ids, vec![PAIR]);
        let data = hex::decode(bound.transaction.data.strip_prefix("0x").unwrap()).unwrap();
        assert_eq!(&data[..4], &[0x38, 0xed, 0x17, 0x39]);
        assert_eq!(data.len(), 4 + 8 * 32);
        assert_eq!(
            &data[4..36],
            &plan.allocation.sell_amount.to_be_bytes().unwrap()
        );
        assert_eq!(&data[36..68], &plan.min_buy_amount.to_be_bytes().unwrap());
        assert_eq!(data[99], 160); // dynamic path offset
        assert_eq!(&data[112..132], &hex::decode(&RECIPIENT[2..]).unwrap());
        assert_eq!(
            &data[132..164],
            &(plan.context.deadline_ms / 1000)
                .to_string()
                .parse::<Amount>()
                .unwrap()
                .to_be_bytes()
                .unwrap()
        );
        assert_eq!(data[195], 2);
        assert_eq!(
            &data[208..228],
            &hex::decode(&plan.context.sell_token[2..]).unwrap()
        );
        assert_eq!(
            &data[240..260],
            &hex::decode(&plan.context.buy_token[2..]).unwrap()
        );
        let mut resealed = bound.clone();
        reseal(&mut resealed);
        assert_eq!(resealed.binding_hash, bound.binding_hash);
    }
}

#[test]
fn consistent_optional_from_token_is_checked_and_supported() {
    let (mut plan, pools) = fixture(false);
    plan.allocation.routes[0].path.edges[0].from_token = Some(TOKEN0.into());
    recommit(&mut plan);
    bind(&plan, &pools).unwrap();
    plan.allocation.routes[0].path.edges[0].from_token = Some(TOKEN1.into());
    recommit(&mut plan);
    assert!(bind(&plan, &pools).is_err());
}

#[test]
fn bound_transaction_tampering_is_rejected_even_with_recomputed_binding_hash() {
    let (plan, pools) = fixture(false);
    let original = bind(&plan, &pools).unwrap();
    for field in ["data", "to", "from", "value", "chain_key"] {
        let mut value = serde_json::to_value(&original).unwrap();
        value["transaction"][field] = match field {
            "data" => "0x38ed1739".into(),
            "to" | "from" => TOKEN1.into(),
            "value" => "1".into(),
            "chain_key" => "evm:1".into(),
            _ => unreachable!(),
        };
        let mut changed: BoundEvmExecution = serde_json::from_value(value).unwrap();
        for rehash in [false, true] {
            if rehash {
                reseal(&mut changed);
            }
            assert!(
                validate_bound_execution(&changed, &pools, &[ROUTER.into()]).is_err(),
                "{field}, reseal={rehash}"
            );
        }
    }
}

#[test]
fn bound_plan_changes_with_new_economic_commitments_cannot_reuse_old_transaction() {
    let (plan, pools) = fixture(false);
    let original = bind(&plan, &pools).unwrap();
    for field in [
        "recipient",
        "caller",
        "slippage",
        "deadline",
        "direction",
        "refund",
        "scope",
        "block",
        "min_out",
        "policy",
    ] {
        let mut changed = original.clone();
        match field {
            "recipient" => changed.plan.context.recipient = CALLER.into(),
            "caller" => changed.plan.context.caller = RECIPIENT.into(),
            "slippage" => changed.plan.context.slippage_bps += 1,
            "deadline" => changed.plan.context.deadline_ms += 1000,
            "direction" => changed.plan.allocation.routes[0].path.edges[0].zero_for_one = false,
            "refund" => changed.plan.allocation.refund = "1".parse().unwrap(),
            "scope" => changed.plan.snapshot.scope_hash = "aa".repeat(32),
            "block" => changed.plan.snapshot.block.hash = format!("0x{}", "aa".repeat(32)),
            "min_out" => changed.plan.min_buy_amount = "1".parse().unwrap(),
            "policy" => changed.plan.economic_policy_hash = "new policy".into(),
            _ => unreachable!(),
        }
        assert!(
            validate_bound_execution(&changed, &pools, &[ROUTER.into()]).is_err(),
            "stale {field}"
        );
        if !matches!(field, "block" | "policy") {
            // New blocks/policies are valid new plans when all bound fields are regenerated.
            if field != "min_out" {
                recommit(&mut changed.plan);
            }
            reseal(&mut changed);
            assert!(
                validate_bound_execution(&changed, &pools, &[ROUTER.into()]).is_err(),
                "recommitted {field}"
            );
        }
    }
    let mut changed = plan;
    changed.context.recipient = CALLER.into();
    recommit(&mut changed);
    bind(&changed, &pools).unwrap(); // changing the recipient legitimately creates a new plan
}

#[test]
fn rehashed_invalid_route_shapes_amounts_and_contexts_fail_binding() {
    let (plan, pools) = fixture(false);
    for case in [
        "empty_routes",
        "split",
        "empty_tokens",
        "extra_token",
        "extra_hop",
        "endpoint",
        "pool",
        "protocol",
        "direction",
        "to",
        "path_in",
        "path_out",
        "route_in",
        "route_out",
        "sell",
        "zero_sell",
        "gross",
        "net",
        "refund",
        "chain",
        "caller",
        "recipient",
        "deadline",
        "zero_deadline",
    ] {
        let mut changed = plan.clone();
        match case {
            "empty_routes" => changed.allocation.routes.clear(),
            "split" => changed
                .allocation
                .routes
                .push(changed.allocation.routes[0].clone()),
            "empty_tokens" => changed.allocation.routes[0].path.tokens.clear(),
            "extra_token" => changed.allocation.routes[0].path.tokens.push(TOKEN0.into()),
            "extra_hop" => {
                let e = changed.allocation.routes[0].path.edges[0].clone();
                changed.allocation.routes[0].path.edges.push(e);
            }
            "endpoint" => changed.allocation.routes[0].path.tokens[0] = TOKEN1.into(),
            "pool" => changed.allocation.routes[0].path.edges[0].pool_id = ROUTER.into(),
            "protocol" => {
                changed.allocation.routes[0].path.edges[0].protocol = "sushiswap-v2".into()
            }
            "direction" => changed.allocation.routes[0].path.edges[0].zero_for_one = false,
            "to" => changed.allocation.routes[0].path.edges[0].to = TOKEN0.into(),
            "path_in" => changed.allocation.routes[0].path.amount_in = "1".parse().unwrap(),
            "path_out" => changed.allocation.routes[0].path.amount_out = "1".parse().unwrap(),
            "route_in" => changed.allocation.routes[0].amount_in = "1".parse().unwrap(),
            "route_out" => changed.allocation.routes[0].amount_out = "1".parse().unwrap(),
            "sell" => changed.allocation.sell_amount = "1".parse().unwrap(),
            "zero_sell" => changed.allocation.sell_amount = Amount::zero(),
            "gross" => changed.allocation.gross_buy_amount = "1".parse().unwrap(),
            "net" => changed.allocation.net_buy_amount = "99999999".parse().unwrap(),
            "refund" => changed.allocation.refund = "1".parse().unwrap(),
            "chain" => {
                changed.context.chain_key = "evm:01".into();
                changed.snapshot.block.chain_key = "evm:01".into();
            }
            "caller" => {
                changed.context.caller = "0x0000000000000000000000000000000000000000".into()
            }
            "recipient" => {
                changed.context.recipient = "0x00000000000000000000000000000000000000AA".into()
            }
            "deadline" => changed.context.deadline_ms = changed.snapshot.block.timestamp_ms - 1,
            "zero_deadline" => changed.context.deadline_ms = 0,
            _ => unreachable!(),
        }
        recommit(&mut changed);
        assert!(bind(&changed, &pools).is_err(), "{case}");
    }
}

#[test]
fn pool_scope_and_snapshot_validation_cover_entire_supplied_state() {
    let (plan, pools) = fixture(false);
    for case in [
        "empty",
        "duplicate",
        "chain",
        "adapter",
        "token_policy",
        "fee",
        "identity",
        "factory",
        "code",
        "schema",
        "generation",
        "adapter_hash",
        "scope_hash",
        "block_hash",
        "parent_hash",
    ] {
        let mut changed = plan.clone();
        let mut state = pools.clone();
        match case {
            "empty" => state.clear(),
            "duplicate" => state.push(state[0].clone()),
            "chain" => state[0].chain_key = "evm:1".into(),
            "adapter" => state[0].adapter_version = "uniswap-v2/2".into(),
            "token_policy" => state[0].token_behavior = "fee-on-transfer".into(),
            "fee" => state[0].fee_numerator = 4,
            "identity" => state[0].v2 = None,
            "factory" => state[0].v2.as_mut().unwrap().factory = ROUTER.into(),
            "code" => state[0].v2.as_mut().unwrap().code_sha256 = "00".repeat(32),
            "schema" => changed.snapshot.schema_version += 1,
            "generation" => changed.snapshot.online_generation = 0,
            "adapter_hash" => changed.snapshot.adapter_set_hash = "aa".repeat(32),
            "scope_hash" => changed.snapshot.scope_hash = "aa".repeat(32),
            "block_hash" => changed.snapshot.block.hash = "arbitrary".into(),
            "parent_hash" => changed.snapshot.block.parent_hash = format!("0x{}", "00".repeat(32)),
            _ => unreachable!(),
        }
        recommit(&mut changed);
        assert!(bind(&changed, &state).is_err(), "{case}");
    }
}

#[test]
fn full_pool_state_hash_is_deterministic_and_detects_untouched_pool_mutation() {
    let (mut plan, mut pools) = fixture(false);
    let mut second = pools[0].clone();
    second.pool_id = "0x0000000000000000000000000000000000000088".into();
    pools.push(second);
    plan.snapshot.scope_hash = scope_hash(&pools).unwrap();
    recommit(&mut plan);
    let bound = bind(&plan, &pools).unwrap();
    pools.reverse();
    let reordered = bind(&plan, &pools).unwrap();
    assert_eq!(bound.pool_state_hash, reordered.pool_state_hash);
    assert_eq!(bound.binding_hash, reordered.binding_hash);
    pools[0].reserve0 = "999999".parse().unwrap();
    let changed = bind(&plan, &pools).unwrap();
    assert_ne!(bound.pool_state_hash, changed.pool_state_hash);
    assert!(validate_bound_execution(&bound, &pools, &[ROUTER.into()]).is_err());
    let mut forged = bound;
    forged.pool_state_hash = changed.pool_state_hash;
    assert!(validate_bound_execution(&forged, &pools, &[ROUTER.into()]).is_err());
}

#[test]
fn executor_identity_is_strict_and_allowlist_is_exact() {
    let (plan, pools) = fixture(false);
    for case in [
        "target",
        "target_zero",
        "factory",
        "factory_zero",
        "code",
        "factory_code",
        "uppercase_code",
        "version",
        "allowlist",
    ] {
        let mut identity = executor();
        let mut allowlist = vec![ROUTER.to_string()];
        match case {
            "target" => identity.target = "0x00000000000000000000000000000000000000AA".into(),
            "target_zero" => {
                identity.target = format!("0x{}", "00".repeat(20));
                allowlist = vec![identity.target.clone()];
            }
            "factory" => identity.factory = CALLER.into(),
            "factory_zero" => identity.factory = format!("0x{}", "00".repeat(20)),
            "code" => identity.code_sha256 = "00".repeat(32),
            "factory_code" => identity.factory_code_sha256 = "bad".into(),
            "uppercase_code" => identity.code_sha256 = "AB".repeat(32),
            "version" => identity.adapter_version = "uniswap-v2-router02/1.0.0".into(),
            "allowlist" => allowlist.clear(),
            _ => unreachable!(),
        }
        assert!(
            bind_v2_execution(&plan, &pools, &identity, &allowlist).is_err(),
            "{case}"
        );
    }
    let bound = bind(&plan, &pools).unwrap();
    assert!(validate_bound_execution(&bound, &pools, &[]).is_err());
    for field in [
        "target",
        "factory",
        "adapter_version",
        "code_sha256",
        "factory_code_sha256",
    ] {
        let mut value = serde_json::to_value(&bound).unwrap();
        value["executor"][field] = match field {
            "target" | "factory" => CALLER.into(),
            "adapter_version" => "uniswap-v2-router02/unpinned".into(),
            _ => "00".repeat(32).into(),
        };
        let mut changed: BoundEvmExecution = serde_json::from_value(value).unwrap();
        reseal(&mut changed);
        assert!(
            validate_bound_execution(&changed, &pools, &[ROUTER.into()]).is_err(),
            "{field}"
        );
    }
}

#[test]
fn bound_fields_and_unknown_fields_cannot_be_smuggled_through_serialization() {
    let (plan, pools) = fixture(false);
    let bound = bind(&plan, &pools).unwrap();
    for field in ["pool_state_hash", "binding_hash", "touched_pool_ids"] {
        let mut value = serde_json::to_value(&bound).unwrap();
        value[field] = if field == "touched_pool_ids" {
            serde_json::json!([TOKEN0])
        } else {
            "aa".repeat(32).into()
        };
        let mut changed: BoundEvmExecution = serde_json::from_value(value).unwrap();
        assert!(
            validate_bound_execution(&changed, &pools, &[ROUTER.into()]).is_err(),
            "{field}"
        );
        if field != "binding_hash" {
            reseal(&mut changed);
            assert!(
                validate_bound_execution(&changed, &pools, &[ROUTER.into()]).is_err(),
                "resealed {field}"
            );
        }
    }
    for location in ["root", "executor", "transaction", "plan"] {
        let mut value = serde_json::to_value(&bound).unwrap();
        let object = if location == "root" {
            &mut value
        } else {
            &mut value[location]
        };
        object["uncommitted_extra"] = true.into();
        assert!(
            serde_json::from_value::<BoundEvmExecution>(value).is_err(),
            "{location}"
        );
    }
}
