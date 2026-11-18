use serde_json::Value;
use xone_domain::{Amount, Pool};

#[test]
#[ignore = "requires this run's independent Go original-code public-bank execution"]
fn current_go_public_v2_original_execution_and_full_after_state_match() {
    let directory = std::path::PathBuf::from(
        std::env::var("XONE_PUBLIC_V2_EXECUTION_OUTPUT")
            .expect("current independently executed original EVM output required"),
    );
    let evidence: Value = serde_json::from_slice(
        &std::fs::read(directory.join("original-public-v2-execution.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(evidence["schema_version"], 1);
    assert_eq!(evidence["consensus_verified"], false);
    assert_eq!(evidence["chain_fees_complete"], false);
    assert_eq!(evidence["public_transactions_submitted"], false);
    assert_eq!(evidence["go_ethereum_version"], "v1.16.7");
    let observed = &evidence["result"];
    assert_eq!(observed["one_more_output_reverts_k"], true);
    assert_eq!(observed["calls"], 3);
    assert!(observed["instructions"].as_u64().unwrap() < 1_000_000);
    let pool: Pool = serde_json::from_value(observed["pool"].clone()).unwrap();
    let expected_after: Pool = serde_json::from_value(observed["next_pool"].clone()).unwrap();
    let input: Amount = observed["input"].as_str().unwrap().parse().unwrap();
    let output: Amount = observed["output"].as_str().unwrap().parse().unwrap();
    let before: Amount = observed["recipient_before"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let after: Amount = observed["recipient_after"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(after.0 - before.0, output.0);
    let actual = xone_protocol_math::v2_complete::quote_exact_in(&pool, &input, false).unwrap();
    assert_eq!(actual.amount_out, output);
    assert_eq!(actual.consumed_in, input);
    assert_eq!(actual.remaining_in, Amount::zero());
    // Compare all physical reserves/raw balances/cumulative prices/time/lock,
    // with the original bank identity retained by derived mathematical states.
    assert_eq!(actual.next_pool, expected_after);
}
