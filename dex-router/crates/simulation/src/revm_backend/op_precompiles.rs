//! OP-specific precompile admission over the pinned cryptographic engines.
//! Enabling an Ethereum BLS implementation alone does not apply OP input caps.
use super::OpFork;
use revm::{
    precompile::bls12_381::{g1_msm, g2_msm},
    primitives::{
        Address, Bytes, Env, Precompile, PrecompileError, PrecompileResult, StatefulPrecompile,
    },
    ContextPrecompile, ContextPrecompiles, Database,
};

#[derive(Clone)]
struct OpPrecompile {
    engine: Precompile,
    address: u8,
    maximum: Option<usize>,
    cap_before_gas: bool,
}
impl OpPrecompile {
    fn required_gas(&self, len: usize) -> u64 {
        let msm = |width: usize, price: u64, table: &[u16]| {
            let k = len / width;
            if k == 0 {
                return 0;
            }
            k as u64 * price * u64::from(table[(k - 1).min(table.len() - 1)]) / 1000
        };
        match self.address {
            8 => 45000 + 34000 * (len / 192) as u64,
            11 => 375,
            12 => msm(160, 12000, &g1_msm::DISCOUNT_TABLE),
            13 => 600,
            14 => msm(288, 22500, &g2_msm::DISCOUNT_TABLE),
            15 => 37700 + 32600 * (len / 384) as u64,
            16 => 5500,
            17 => 23800,
            _ => unreachable!("fixed OP precompile registry"),
        }
    }
}
impl StatefulPrecompile for OpPrecompile {
    fn call(&self, input: &Bytes, gas: u64, env: &Env) -> PrecompileResult {
        if self.cap_before_gas && self.maximum.is_some_and(|max| input.len() > max) {
            return Err(PrecompileError::Other("OP precompile input limit".into()).into());
        }
        // Native op-geth charges RequiredGas before checking input validity.
        // Enforce caps before entering the native crypto implementation.
        if gas < self.required_gas(input.len()) {
            return Err(PrecompileError::OutOfGas.into());
        }
        if self.maximum.is_some_and(|maximum| input.len() > maximum) {
            return Err(PrecompileError::Other("OP precompile input limit".into()).into());
        }
        self.engine.call_ref(input, gas, env)
    }
}

pub(super) fn load<DB: Database, EXT>(fork: OpFork) -> ContextPrecompiles<DB> {
    use revm::optimism::load_precompiles;
    use revm::primitives::*;
    let mut table = match fork {
        OpFork::Regolith => load_precompiles::<RegolithSpec, EXT, DB>(),
        OpFork::Canyon => load_precompiles::<CanyonSpec, EXT, DB>(),
        OpFork::Ecotone => load_precompiles::<EcotoneSpec, EXT, DB>(),
        OpFork::Fjord => load_precompiles::<FjordSpec, EXT, DB>(),
        OpFork::Granite => load_precompiles::<GraniteSpec, EXT, DB>(),
        OpFork::Holocene => load_precompiles::<HoloceneSpec, EXT, DB>(),
        OpFork::Isthmus | OpFork::Jovian | OpFork::Karst => {
            load_precompiles::<IsthmusSpec, EXT, DB>()
        }
    };
    install(fork, &mut table);
    table
}

pub(super) fn install<DB: Database>(fork: OpFork, table: &mut ContextPrecompiles<DB>) {
    if !matches!(fork, OpFork::Isthmus | OpFork::Jovian | OpFork::Karst) {
        return;
    }
    let karst = fork == OpFork::Karst;
    let jovian = matches!(fork, OpFork::Jovian | OpFork::Karst);
    for (address, maximum) in [
        (
            8,
            Some(if karst {
                57600
            } else if jovian {
                81984
            } else {
                112687
            }),
        ),
        (11, None),
        (12, Some(if jovian { 288960 } else { 513760 })),
        (13, None),
        (14, Some(if jovian { 278784 } else { 488448 })),
        (15, Some(if jovian { 156672 } else { 235008 })),
        (16, None),
        (17, None),
    ] {
        let key = Address::with_last_byte(address);
        let Some(ContextPrecompile::Ordinary(engine)) = table.to_mut().get(&key) else {
            unreachable!("OP build requires KZG/BLS cryptographic features");
        };
        let engine = engine.clone();
        table.to_mut().insert(
            key,
            Precompile::new_stateful(OpPrecompile {
                engine,
                address,
                maximum,
                cap_before_gas: karst,
            })
            .into(),
        );
    }
    if karst {
        table.to_mut().insert(
            Address::with_last_byte(5),
            Precompile::Standard(super::karst_precompiles::modexp_osaka).into(),
        );
        table.to_mut().insert(
            revm::primitives::address!("0000000000000000000000000000000000000100"),
            Precompile::Standard(super::karst_precompiles::p256_osaka).into(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use revm::{db::EmptyDB, primitives::*, InnerEvmContext};
    use serde_json::Value;
    fn table(fork: OpFork) -> ContextPrecompiles<EmptyDB> {
        load::<EmptyDB, ()>(fork)
    }
    #[test]
    fn op_precompile_registries_include_required_crypto_and_preserve_fork_admission() {
        for (fork, count) in [
            (OpFork::Regolith, 9),
            (OpFork::Canyon, 9),
            (OpFork::Ecotone, 10),
            (OpFork::Fjord, 11),
            (OpFork::Granite, 11),
            (OpFork::Holocene, 11),
            (OpFork::Isthmus, 18),
            (OpFork::Jovian, 18),
        ] {
            let mut t = table(fork);
            assert_eq!(t.addresses().len(), count, "{fork:?}");
            assert_eq!(
                t.contains(&Address::with_last_byte(11)),
                matches!(fork, OpFork::Isthmus | OpFork::Jovian)
            );
            // KZG is callable from Ecotone onward; a missing backend must never
            // silently turn it into an empty account or a fatal host error.
            if count >= 10 {
                let mut context = InnerEvmContext::new(EmptyDB::default());
                let out = t
                    .call(
                        &Address::with_last_byte(10),
                        &Bytes::new(),
                        50000,
                        &mut context,
                    )
                    .unwrap();
                assert!(matches!(
                    out,
                    Err(PrecompileErrors::Error(
                        PrecompileError::BlobInvalidInputLength
                    ))
                ));
            }
        }
    }
    #[test]
    #[ignore = "requires the pinned original OP precompile corpus"]
    fn op_precompiles_match_original_corpus_caps_gas_and_fork_dispatch() {
        let v: Value = serde_json::from_slice(
            &std::fs::read(std::env::var("XONE_OP_PRECOMPILE_VECTORS").unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(v["passed"], true);
        let mut cases = 0;
        for row in v["cases"].as_array().unwrap() {
            let fork = match row["input"]["fork"].as_str().unwrap() {
                "regolith" => OpFork::Regolith,
                "canyon" => OpFork::Canyon,
                "ecotone" => OpFork::Ecotone,
                "fjord" => OpFork::Fjord,
                "granite" => OpFork::Granite,
                "holocene" => OpFork::Holocene,
                "isthmus" => OpFork::Isthmus,
                "jovian" => OpFork::Jovian,
                _ => panic!(),
            };
            let mut t = table(fork);
            let address: Address = row["input"]["address"].as_str().unwrap().parse().unwrap();
            let bytes = Bytes::from(hex::decode(row["input"]["data"].as_str().unwrap()).unwrap());
            let gas = row["input"]["gas"].as_u64().unwrap();
            let mut context = InnerEvmContext::new(EmptyDB::default());
            let result = t.call(&address, &bytes, gas, &mut context);
            assert_eq!(
                result.is_some(),
                row["present"].as_bool().unwrap(),
                "{}",
                row["input"]["id"]
            );
            if let Some(result) = result {
                assert_eq!(
                    result.is_ok(),
                    row["success"].as_bool().unwrap(),
                    "{}: {result:?}",
                    row["input"]["id"]
                );
                assert_eq!(
                    matches!(
                        &result,
                        Err(PrecompileErrors::Error(PrecompileError::OutOfGas))
                    ),
                    row["out_of_gas"].as_bool().unwrap(),
                    "{}",
                    row["input"]["id"]
                );
                if let Ok(output) = result {
                    assert_eq!(
                        hex::encode(output.bytes),
                        row["output"].as_str().unwrap(),
                        "{}",
                        row["input"]["id"]
                    );
                    assert_eq!(
                        output.gas_used,
                        row["required_gas"].as_u64().unwrap(),
                        "{}",
                        row["input"]["id"]
                    );
                }
            }
            cases += 1;
        }
        assert!(cases > 1000, "original corpus incomplete");
    }
}
