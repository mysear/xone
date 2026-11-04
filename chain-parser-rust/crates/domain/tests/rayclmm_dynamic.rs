use xone_domain::{
    raydium_clmm::{RaydiumCLMMBitmapExtension, RaydiumCLMMState, PROGRAM_ID},
    Pool,
};
fn pda(seeds: &[&[u8]]) -> (String, u32) {
    use sha2::{Digest, Sha256};
    let program = bs58::decode(PROGRAM_ID).into_vec().unwrap();
    for bump in (0..=255u8).rev() {
        let mut hash = Sha256::new();
        for seed in seeds {
            hash.update(seed)
        }
        hash.update([bump]);
        hash.update(&program);
        hash.update(b"ProgramDerivedAddress");
        let raw: [u8; 32] = hash.finalize().into();
        if curve25519_dalek::edwards::CompressedEdwardsY(raw)
            .decompress()
            .is_none()
        {
            return (bs58::encode(raw).into_string(), u32::from(bump));
        }
    }
    panic!("PDA")
}
fn bytes(s: &str) -> Vec<u8> {
    bs58::decode(s).into_vec().unwrap()
}
fn seeded(seed: u32) -> Pool {
    let mut p: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/rayclmm-state.json")).unwrap();
    p.protocol = "raydium-clmm-custom-seed".into();
    p.adapter_version = "raydium-clmm/6d7a0f82-spl-custom-seed-1".into();
    let s = p.raydium_clmm.as_mut().unwrap();
    s.seed_index = seed;
    let (key, bump) = pda(&[
        b"pool",
        &bytes(&s.amm_config),
        &bytes(&p.token0),
        &bytes(&p.token1),
        &(seed as u16).to_le_bytes(),
    ]);
    p.pool_id = key;
    s.pool_bump = bump;
    let pool = bytes(&p.pool_id);
    s.vault0 = pda(&[b"pool_vault", &pool, &bytes(&p.token0)]).0;
    s.vault1 = pda(&[b"pool_vault", &pool, &bytes(&p.token1)]).0;
    s.observation = pda(&[b"observation", &pool]).0;
    s.bitmap_extension = Some(RaydiumCLMMBitmapExtension {
        address: pda(&[b"pool_tick_array_bitmap_extension", &pool]).0,
        positive: vec![0; 112],
        negative: vec![0; 112],
    });
    for a in &mut s.tick_arrays {
        a.address = pda(&[b"tick_array", &pool, &a.start_tick_index.to_be_bytes()]).0;
    }
    p
}

fn dynamic() -> Pool {
    let mut p = seeded(1);
    p.protocol = "raydium-clmm-dynamic".into();
    p.adapter_version = "raydium-clmm/6d7a0f82-spl-dynamic-1".into();
    let s = p.raydium_clmm.as_mut().unwrap();
    s.dynamic_fee = Some(xone_domain::raydium_clmm::RaydiumCLMMDynamicFee {
        filter_period: 10,
        decay_period: 60,
        reduction_factor: 5000,
        dynamic_fee_control: 1000,
        max_volatility_accumulator: 1000000,
        tick_spacing_index_reference: 0,
        volatility_reference: 20000,
        volatility_accumulator: 30000,
        last_update_timestamp: s.bank_unix_timestamp,
    });
    p
}
#[test]
fn dynamic_state_profile_width_and_wire_are_explicit() {
    let p = dynamic();
    p.validate().unwrap();
    let s = p.raydium_clmm.as_ref().unwrap();
    assert_eq!(
        &RaydiumCLMMState::from_proto(s.to_proto().unwrap()).unwrap(),
        s
    );
    let mut p = p.clone();
    p.raydium_clmm
        .as_mut()
        .unwrap()
        .dynamic_fee
        .as_mut()
        .unwrap()
        .last_update_timestamp += 1;
    p.validate().unwrap();
}
#[test]
fn dynamic_rejects_invalid_configuration_and_old_profile_contamination() {
    for mode in [
        "missing",
        "filter",
        "decay",
        "reduction",
        "control",
        "maxwidth",
        "reference",
        "accumulator",
        "index",
        "timestamp",
        "oldprofile",
    ] {
        let mut p = dynamic();
        let s = p.raydium_clmm.as_mut().unwrap();
        let d = s.dynamic_fee.as_mut().unwrap();
        match mode {
            "missing" => s.dynamic_fee = None,
            "filter" => d.filter_period = 0,
            "decay" => d.decay_period = 10,
            "reduction" => d.reduction_factor = 10000,
            "control" => d.dynamic_fee_control = 100000,
            "maxwidth" => d.max_volatility_accumulator = u32::MAX,
            "reference" => d.volatility_reference = 1000001,
            "accumulator" => d.volatility_accumulator = 1000001,
            "index" => d.tick_spacing_index_reference = i32::MAX,
            "timestamp" => d.last_update_timestamp = u32::MAX as u64 + 1,
            _ => {
                p.protocol = "raydium-clmm-custom-seed".into();
                p.adapter_version = "raydium-clmm/6d7a0f82-spl-custom-seed-1".into()
            }
        }
        assert!(p.validate().is_err(), "{mode}")
    }
    let p = seeded(1);
    assert!(serde_json::to_value(p.raydium_clmm)
        .unwrap()
        .get("dynamic_fee")
        .is_none());
}
