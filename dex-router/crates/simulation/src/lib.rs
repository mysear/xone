pub mod evm_fixture;
pub mod revm_backend;
pub mod worker;
use serde::{Deserialize, Serialize};
use serde_json::json;
use xone_domain::{err, Amount, Result};
use xone_execution_plan::{Plan, Transaction};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evidence {
    pub level: String,
    pub success: bool,
    pub plan_hash: String,
    pub block_hash: String,
    pub simulator_version: String,
    pub actual_buy_amount: Option<Amount>,
    pub evidence_id: String,
}
/// A successful eth_call is diagnostic evidence. It does not prove recipient balance changes.
/// VERIFIED remains disabled until an independent balance-delta execution backend is installed.
pub fn verify(_plan: &Plan) -> Result<Evidence> {
    Err(err("INDEPENDENT_SIMULATION_UNAVAILABLE"))
}
pub async fn check_evm_call(
    client: &reqwest::Client,
    url: &str,
    transaction: &Transaction,
    plan: &Plan,
    max_response: usize,
) -> Result<Evidence> {
    let mut response=client.post(url).json(&json!({"jsonrpc":"2.0","id":1,"method":"eth_call","params":[{"from":transaction.from,"to":transaction.to,"data":transaction.data,"value":"0x0"},{"blockHash":plan.snapshot.block.hash,"requireCanonical":true}]})).send().await.map_err(|_|err("SIMULATION_RPC_NETWORK"))?;
    if !response.status().is_success() {
        return Err(err("SIMULATION_RPC_HTTP"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| err("SIMULATION_RPC_NETWORK"))?
    {
        if bytes.len() + chunk.len() > max_response {
            return Err(err("SIMULATION_RESPONSE_LIMIT"));
        }
        bytes.extend(chunk);
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| err("SIMULATION_RPC_RESULT"))?;
    if value["id"] != 1
        || value["jsonrpc"] != "2.0"
        || value.get("error").is_some()
        || value["result"].as_str().is_none()
    {
        return Err(err("SIMULATION_CALL_FAILED"));
    }
    let evidence_id = xone_domain::hash(
        &json!({"plan_hash":plan.plan_hash,"result":value["result"],"block_hash":plan.snapshot.block.hash}),
    )?;
    Ok(Evidence {
        level: "RPC_CALL_CHECKED".into(),
        success: true,
        plan_hash: plan.plan_hash.clone(),
        block_hash: plan.snapshot.block.hash.clone(),
        simulator_version: "evm-jsonrpc/1".into(),
        actual_buy_amount: None,
        evidence_id,
    })
}
