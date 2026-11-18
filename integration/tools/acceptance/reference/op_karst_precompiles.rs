//! I/O only. spec.rs and precompiles.rs are unchanged original OP source files.
#![allow(dead_code)]
mod precompiles;
mod spec;
use revm::precompile::{PrecompileHalt, PrecompileStatus};
use revm::primitives::Address;
use serde::{Deserialize, Serialize};
pub use spec::OpSpecId;
use std::io::Read;
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Input {
    id: String,
    address: String,
    data: String,
    gas: u64,
}
fn main() {
    let mut text = String::new();
    std::io::stdin()
        .take(128 * 1024 * 1024)
        .read_to_string(&mut text)
        .unwrap();
    let inputs: Vec<Input> = serde_json::from_str(&text).unwrap();
    assert!(!inputs.is_empty() && inputs.len() <= 15000);
    let table = precompiles::karst();
    let mut rows = Vec::new();
    for input in inputs {
        let address: Address = input.address.parse().unwrap();
        let bytes = hex::decode(&input.data).unwrap();
        assert!(bytes.len() <= 1_000_000);
        let result = table
            .get(&address)
            .expect("original Karst registry entry")
            .execute(&bytes, input.gas, 0)
            .expect("original precompile fatal error");
        let success = result.status == PrecompileStatus::Success;
        rows.push(serde_json::json!({"input":input,"success":success,"out_of_gas":matches!(result.status, PrecompileStatus::Halt(PrecompileHalt::OutOfGas)),"gas_used":result.gas_used,"output":hex::encode(result.bytes),"status":format!("{:?}",result.status)}));
    }
    serde_json::to_writer(std::io::stdout(), &rows).unwrap();
}
