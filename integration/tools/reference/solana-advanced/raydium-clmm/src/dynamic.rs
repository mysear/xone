//! Typed adapters only: DynamicFeeInfo and all dynamic swap arithmetic execute upstream.
use super::*;
pub const DYNAMIC_PROFILE: &str = "raydium-clmm-dynamic";
pub const DYNAMIC_ADAPTER: &str = "raydium-clmm/6d7a0f82-spl-dynamic-1";
/// Explicit snapshot taken on 2026-10-29; pass --timestamp to generate a fresh native snapshot.
pub const DYNAMIC_TIME_ANCHOR: u64 = 1_793_245_486;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RaydiumCLMMDynamicFee {
    pub filter_period: u32,
    pub decay_period: u32,
    pub reduction_factor: u32,
    pub dynamic_fee_control: u32,
    pub max_volatility_accumulator: u32,
    pub tick_spacing_index_reference: i32,
    pub volatility_reference: u32,
    pub volatility_accumulator: u32,
    pub last_update_timestamp: u64,
}
impl RaydiumCLMMDynamicFee {
    pub(super) fn original(&self) -> DynamicFeeInfo {
        DynamicFeeInfo {
            filter_period: u16::try_from(self.filter_period).expect("validated filter"),
            decay_period: u16::try_from(self.decay_period).expect("validated decay"),
            reduction_factor: u16::try_from(self.reduction_factor).expect("validated reduction"),
            dynamic_fee_control: self.dynamic_fee_control,
            max_volatility_accumulator: self.max_volatility_accumulator,
            tick_spacing_index_reference: self.tick_spacing_index_reference,
            volatility_reference: self.volatility_reference,
            volatility_accumulator: self.volatility_accumulator,
            last_update_timestamp: self.last_update_timestamp,
            padding: [0; 46],
        }
    }
    pub(super) fn from_original(f: DynamicFeeInfo) -> Self {
        Self {
            filter_period: f.filter_period as u32,
            decay_period: f.decay_period as u32,
            reduction_factor: f.reduction_factor as u32,
            dynamic_fee_control: f.dynamic_fee_control,
            max_volatility_accumulator: f.max_volatility_accumulator,
            tick_spacing_index_reference: f.tick_spacing_index_reference,
            volatility_reference: f.volatility_reference,
            volatility_accumulator: f.volatility_accumulator,
            last_update_timestamp: f.last_update_timestamp,
        }
    }
}
pub(super) fn validate_dynamic(p: &Pool) -> std::result::Result<(), String> {
    let s = &p.raydium_clmm;
    let f = s.dynamic_fee.as_ref().ok_or("dynamic fee required")?;
    if s.tick_spacing == 0
        || s.tick_spacing > 1000
        || f.filter_period > 65535
        || f.decay_period > 65535
        || f.reduction_factor > 65535
        || !DynamicFeeInfo::validate_params(
            s.tick_spacing as u16,
            f.filter_period as u16,
            f.decay_period as u16,
            f.reduction_factor as u16,
            f.dynamic_fee_control,
            f.max_volatility_accumulator,
        )
        || f.volatility_reference > f.max_volatility_accumulator
        || f.volatility_accumulator > f.max_volatility_accumulator
        || f.last_update_timestamp > u32::MAX as u64
        || !(tick_math::MIN_TICK.div_euclid(s.tick_spacing as i32)
            ..=tick_math::MAX_TICK.div_euclid(s.tick_spacing as i32))
            .contains(&f.tick_spacing_index_reference)
    {
        return Err("dynamic parameter / reference / timestamp bound".into());
    }
    Ok(())
}
/// This is intentionally an admission bound, never a claim about measured native steps.
/// Every swap step finishes a spacing interval, a tick boundary, or the final partial fill.
/// Count the complete proof window plus tick boundaries and a final step; do not put
/// rejected admission probes into the mathematical differential corpus.
pub(super) fn dynamic_admission(p: &Pool) -> std::result::Result<(), String> {
    let s = &p.raydium_clmm;
    let first = s
        .tick_arrays
        .first()
        .ok_or("dynamic arrays")?
        .start_tick_index;
    let last = s
        .tick_arrays
        .last()
        .ok_or("dynamic arrays")?
        .start_tick_index;
    let spacing_steps = ((last as i64 - first as i64).unsigned_abs() / s.tick_spacing as u64) + 60;
    let ticks: u64 = s.tick_arrays.iter().map(|a| a.ticks.len() as u64).sum();
    if spacing_steps + ticks + 2 > s.max_steps as u64 {
        return Err("dynamic spacing admission (conservative, not runtime budget)".into());
    }
    Ok(())
}

pub fn dynamic_pool(center_index: i32, mode: u32, seed: u32, spacing: u32, timestamp: u64) -> Pool {
    let mut p = if seed == 0 {
        bitmap_pool(center_index, mode)
    } else {
        seed_pool(center_index, mode, seed)
    };
    p.protocol = DYNAMIC_PROFILE.into();
    p.adapter_version = DYNAMIC_ADAPTER.into();
    let s = &mut p.raydium_clmm;
    s.bank_unix_timestamp = timestamp;
    s.tick_spacing = spacing;
    for a in &mut s.tick_arrays {
        a.start_tick_index *= spacing as i32;
        a.address = pda(
            &[
                TICK_ARRAY_SEED.as_bytes(),
                pk(&p.pool_id).as_ref(),
                &a.start_tick_index.to_be_bytes(),
            ],
            &pk(PROGRAM),
        )
        .0;
        for t in &mut a.ticks {
            t.tick *= spacing as i32;
        }
    }
    s.tick_current *= spacing as i32;
    s.sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(s.tick_current)
        .unwrap()
        .to_string();
    s.dynamic_fee = Some(RaydiumCLMMDynamicFee {
        filter_period: 10,
        decay_period: 120,
        reduction_factor: 5000,
        dynamic_fee_control: 10000,
        max_volatility_accumulator: 1_000_000,
        tick_spacing_index_reference: s.tick_current.div_euclid(spacing as i32),
        volatility_reference: 0,
        volatility_accumulator: 0,
        last_update_timestamp: 0,
    });
    rebuild(&mut p);
    p
}
pub fn dynamic_next_bank_pool(timestamp: u64) -> Pool {
    let mut p = dynamic_pool(512, 0, 1, 1, timestamp + 1);
    p.raydium_clmm.bank_slot += 1;
    let f = p.raydium_clmm.dynamic_fee.as_mut().unwrap();
    f.dynamic_fee_control = 90000;
    f.max_volatility_accumulator = 2_000_000;
    f.volatility_reference = 800_000;
    f.volatility_accumulator = 900_000;
    // Both process banks start beyond decay. Original SwapState::new resets the
    // reference at the actual Clock; later Clock changes preserve exact amounts.
    f.last_update_timestamp = 0;
    p
}
fn vectors(p: &Pool, prefix: &str) -> Value {
    json!([false,true].into_iter().flat_map(|d| [1_000_000,2_000_000_000].into_iter().map(move |a|
        json!({"name":format!("{prefix}-{}-{a}", if d {"zero-for-one"} else {"one-for-zero"}),"input":a.to_string(),"zero_for_one":d,"expected":expected(p,a,d)})
    )).collect::<Vec<_>>())
}
pub fn dynamic_fixture(timestamp: u64) -> Value {
    let p = dynamic_pool(512, 0, 1, 1, timestamp);
    let next = dynamic_next_bank_pool(timestamp);
    let mut f = fixture_from_bitmap(&p);
    f["metadata"]["next_bank_fixture"] = fixture_from_bitmap(&next);
    f["metadata"]["base_vectors"] = vectors(&p, "dynamic-base-exact-fixture");
    f["metadata"]["next_bank_vectors"] = vectors(&next, "dynamic-next-bank-exact-fixture");
    f["metadata"]["native_time_anchor"] = json!(timestamp);
    f["metadata"]["dynamic_fee_raw_offset"] = json!([1096, 1176]);
    f["metadata"]["dynamic_fee_fields_offset"] = json!([1096, 1130]);
    f["metadata"]["reserved_zero_offset"] = json!([1130, 1544]);
    f["metadata"]["clock_strategy"] = json!({"held_unix_snapshot":timestamp,"source_date":"2026-10-29","initial_last_update_timestamp":0,"base_amount_stability":"Every later u32 bank time beyond decay resets reference before swap; only timestamp/observations differ. Process amount comparisons may use current Clock.","next_bank_amount_stability":"last_update_timestamp=0 resets reference at actual Clock; control/max accumulator changes alter fees after spacing movement.","profile_freshness":"Process rewrites Clock and decoded bank timestamp to actual current source time with configMaxAge=180s, and asserts current-last_update_timestamp>=decay_period in both banks. Compare amounts with the held native fixture template; full poststates remain fixed-time differential evidence.","refresh":"generate-dynamic/dynamic-fixture --timestamp <explicit-current-unix> regenerate from actual native Clock; no wall clock read during generation"});
    f["metadata"]["time_policy"] = json!({"native_time_anchor":timestamp,"process_clock":"actual current bank Clock and matching block timestamp","config_max_age_seconds":180,"required_precondition":"bank_unix_timestamp.saturating_sub(last_update_timestamp) >= decay_period, both banks","base_last_update_timestamp":0,"next_bank_last_update_timestamp":0,"compare":"exact amounts invariant; full Pool dynamic timestamps and observation timestamps depend on bank Clock","native_time_invariance_probes":dynamic_time_probes(timestamp)});
    f["metadata"]["next_bank_change"] = json!({"seed_index":[1,1],"dynamic_fee_control":[10000,90000],"max_volatility_accumulator":[1000000,2000000],"volatility_reference":[0,800000],"volatility_accumulator":[0,900000],"last_update_timestamp":[0,0],"reset_semantics":"reference inputs reset beyond decay; control/max changes persist","same_account_window":true});
    f["metadata"]["pda_derivation"] = json!("Original PoolState::set_seed_index/key; seed1 appends u16 LE; dependent PDA seeds unchanged");
    f
}
pub fn dynamic_time_probes(timestamp: u64) -> Value {
    let mut probes = vec![];
    for (label, p) in [
        ("base", dynamic_pool(512, 0, 1, 1, timestamp)),
        ("next-bank", dynamic_next_bank_pool(timestamp)),
    ] {
        for d in [false, true] {
            for a in [1_000_000u64, 2_000_000_000] {
                let mut later = p.clone();
                later.raydium_clmm.bank_unix_timestamp += 3600;
                probes.push(json!({"name":format!("{label}-time-invariant-dir{d}-{a}"),"pool":p,"later_pool":later,"input":a.to_string(),"zero_for_one":d,"expected":expected(&p,a,d),"later_expected":expected(&later,a,d)}));
            }
        }
    }
    json!(probes)
}
fn push(cases: &mut Vec<Value>, name: String, p: &Pool, amount: u64, d: bool) {
    cases.push(json!({"name":name,"pool":p,"input":amount.to_string(),"zero_for_one":d,"expected":expected(p,amount,d)}));
}
pub fn generate_dynamic(timestamp: u64) -> Value {
    let base = dynamic_pool(512, 0, 1, 1, timestamp);
    let mut cases = vec![];
    // The complete fee/seed/direction matrix, both signs and independent exact-size quotes.
    for seed in [0, 1, 255, 256, 65535] {
        for mode in 0..=2 {
            for center in [512, -512] {
                for d in [false, true] {
                    for amount in [1_000_000, 2_000_000_000] {
                        let p = dynamic_pool(center, mode, seed, 1, timestamp);
                        push(
                            &mut cases,
                            format!("seed{seed}-mode{mode}-index{center}-dir{d}-{amount}"),
                            &p,
                            amount,
                            d,
                        );
                    }
                }
            }
        }
    }
    for mode in 0..=2 {
        for d in [false, true] {
            for elapsed in [0, 1, 9, 10, 11, 119, 120, 121] {
                let mut p = dynamic_pool(-512, mode, 1, 1, timestamp);
                let f = p.raydium_clmm.dynamic_fee.as_mut().unwrap();
                f.last_update_timestamp = timestamp - elapsed;
                f.volatility_reference = 12345;
                f.volatility_accumulator = 54321;
                push(
                    &mut cases,
                    format!("time-filter-decay-{elapsed}-mode{mode}-dir{d}"),
                    &p,
                    2_000_000_000,
                    d,
                );
            }
        }
    }
    for mode in 0..=2 {
        for d in [false, true] {
            let mut p = base.clone();
            p.raydium_clmm.collect_fee_on = mode;
            let f = p.raydium_clmm.dynamic_fee.as_mut().unwrap();
            f.last_update_timestamp = timestamp + 100;
            f.volatility_reference = 12345;
            f.volatility_accumulator = 54321;
            push(
                &mut cases,
                format!("future-last-timestamp-saturates-mode{mode}-dir{d}"),
                &p,
                2_000_000_000,
                d,
            );
        }
    }
    for spacing in [1, 2, 3, 10, 60, 1000] {
        for center in [-5, 0, 5] {
            for d in [false, true] {
                let mut p = dynamic_pool(center, 0, 0, spacing, timestamp);
                // Nonaligned positive/negative current tick exercises original floor division.
                let tick =
                    p.raydium_clmm.tick_current + if spacing > 1 { spacing as i32 - 1 } else { 0 };
                reset_price(&mut p, tick);
                push(
                    &mut cases,
                    format!("spacing{spacing}-floor-negative{center}-dir{d}"),
                    &p,
                    2_000_000_000,
                    d,
                );
            }
        }
    }
    for control in [1, 999, 10000, 99999] {
        for max in [0, 10000, 1000000, u32::MAX] {
            for d in [false, true] {
                let mut p = base.clone();
                let f = p.raydium_clmm.dynamic_fee.as_mut().unwrap();
                f.dynamic_fee_control = control;
                f.max_volatility_accumulator = max;
                f.last_update_timestamp = timestamp + 1;
                f.volatility_reference = max;
                f.volatility_accumulator = max;
                push(
                    &mut cases,
                    format!("ceil-control{control}-maxacc{max}-cap-dir{d}"),
                    &p,
                    2_000_000_000,
                    d,
                );
            }
        }
    }
    for center in [511, -512, 1024, -1024, 7168, -7168] {
        for d in [false, true] {
            let p = dynamic_pool(center, 0, 1, 1, timestamp);
            push(
                &mut cases,
                format!("groupcross-index{center}-dir{d}"),
                &p,
                if center.abs() > 7000 {
                    1_000_000
                } else {
                    30_000_000_000
                },
                d,
            );
        }
    }
    for mode in 0..=2 {
        for d in [false, true] {
            for kind in [
                "zeroL",
                "growth-wrap",
                "raw-input-overflow",
                "raw-output-underflow",
                "protocol-counter-overflow",
                "fund-counter-overflow",
                "u64-max-partial",
                "u128-max",
            ] {
                let mut p = dynamic_pool(0, mode, 1, 1, timestamp);
                let s = &mut p.raydium_clmm;
                let amount = match kind {
                    "zeroL" => {
                        s.liquidity = "0".into();
                        for a in &mut s.tick_arrays {
                            for t in &mut a.ticks {
                                t.liquidity_net =
                                    if d { "-1000000000000" } else { "1000000000000" }.into();
                            }
                        }
                        2_000_000_000
                    }
                    "growth-wrap" => {
                        s.fee_growth_global0 = (u128::MAX - 1).to_string();
                        s.fee_growth_global1 = (u128::MAX - 1).to_string();
                        2_000_000_000
                    }
                    "raw-input-overflow" => {
                        if d {
                            s.raw_vault0 = u64::MAX;
                        } else {
                            s.raw_vault1 = u64::MAX;
                        }
                        1_000_000
                    }
                    "raw-output-underflow" => {
                        if d {
                            s.raw_vault1 = 0;
                        } else {
                            s.raw_vault0 = 0;
                        }
                        1_000_000
                    }
                    "protocol-counter-overflow" => {
                        s.protocol_fees0 = u64::MAX;
                        s.protocol_fees1 = u64::MAX;
                        1_000_000
                    }
                    "fund-counter-overflow" => {
                        s.fund_fees0 = u64::MAX;
                        s.fund_fees1 = u64::MAX;
                        1_000_000
                    }
                    "u64-max-partial" => u64::MAX,
                    _ => {
                        s.liquidity = u128::MAX.to_string();
                        2_000_000_000
                    }
                };
                push(
                    &mut cases,
                    format!("{kind}-mode{mode}-dir{d}"),
                    &p,
                    amount,
                    d,
                );
            }
        }
    }
    for mode in 0..=2 {
        for d in [false, true] {
            for kind in [
                "observation-wrap",
                "selected-missing-proof-array",
                "partial-window-exhaustion",
                "basefee-cap",
                "ceil-first-unit",
            ] {
                let mut p = dynamic_pool(0, mode, 1, 1, timestamp);
                let amount = match kind {
                    "observation-wrap" => {
                        let s = &mut p.raydium_clmm;
                        s.observation_initialized = true;
                        s.observation_index = 99;
                        s.observations[99].block_timestamp = (timestamp - 30) as u32;
                        s.observations[99].tick_cumulative = -123;
                        2_000_000_000
                    }
                    "selected-missing-proof-array" => {
                        p.raydium_clmm.tick_arrays.remove(if d { 2 } else { 4 });
                        30_000_000_000
                    }
                    "partial-window-exhaustion" => 1_000_000_000_000,
                    "basefee-cap" => {
                        p.raydium_clmm.trade_fee_rate = 200000;
                        p.fee_numerator = 200000;
                        2_000_000_000
                    }
                    _ => {
                        let f = p.raydium_clmm.dynamic_fee.as_mut().unwrap();
                        f.dynamic_fee_control = 1;
                        f.volatility_reference = 1;
                        f.volatility_accumulator = 1;
                        f.last_update_timestamp = timestamp + 1;
                        1_000_000
                    }
                };
                push(
                    &mut cases,
                    format!("{kind}-mode{mode}-dir{d}"),
                    &p,
                    amount,
                    d,
                );
            }
        }
    }
    // Fixed-time full-Pool copies of each metamorphic clock probe.
    for probe in dynamic_time_probes(timestamp).as_array().unwrap() {
        let p: Pool = serde_json::from_value(probe["later_pool"].clone()).unwrap();
        push(
            &mut cases,
            format!("{}-later-bank", probe["name"].as_str().unwrap()),
            &p,
            probe["input"].as_str().unwrap().parse().unwrap(),
            probe["zero_for_one"].as_bool().unwrap(),
        );
    }
    let next = dynamic_next_bank_pool(timestamp);
    for (p, prefix) in [
        (&base, "dynamic-base-exact-fixture"),
        (&next, "dynamic-next-bank-exact-fixture"),
    ] {
        for v in vectors(p, prefix).as_array().unwrap() {
            push(
                &mut cases,
                v["name"].as_str().unwrap().into(),
                p,
                v["input"].as_str().unwrap().parse().unwrap(),
                v["zero_for_one"].as_bool().unwrap(),
            );
        }
    }
    let mut sequences = vec![];
    for mode in 0..=2 {
        for directions in [[true, true], [false, false], [true, false], [false, true]] {
            let initial = dynamic_pool(0, mode, 1, 1, timestamp);
            let mut p = initial.clone();
            let mut steps = vec![];
            for d in directions {
                let e = expected(&p, 2_000_000_000, d);
                assert!(e.get("reverted").is_none());
                steps.push(json!({"input":"2000000000","zero_for_one":d,"expected":e}));
                p = serde_json::from_value(e["next_pool"].clone()).unwrap();
            }
            sequences.push(json!({"name":format!("mode{mode}-directions{directions:?}"),"pool":initial,"steps":steps,"expected_final_pool":p}));
        }
    }
    let mut admission = vec![];
    for budget in [1, 22, 420] {
        for d in [false, true] {
            let mut p = base.clone();
            p.raydium_clmm.max_steps = budget;
            let reason = quote(&p, "2000000000", d).err().expect("admission bound");
            admission.push(json!({"name":format!("spacing-admission-budget{budget}-dir{d}"),"pool":p,"input":"2000000000","zero_for_one":d,"expected":{"reverted":true},"reason_contains":"dynamic spacing admission","native_reason":reason,"relaxed_pool":base,"relaxed_expected":expected(&base,2_000_000_000,d),"domain":"conservative admission only; excluded from mathematical cases"}));
        }
    }
    json!({"schema_version":1,"reference":{"revision":REVISION,"profile":DYNAMIC_PROFILE,"adapter_version":DYNAMIC_ADAPTER,"native_time_anchor":timestamp,"source_function":"original DynamicFeeInfo / SwapState / swap_internal / RefMut / Clock","business_math_imported":false,"sbf_transaction_executed":false,"token_program_executed":false,"anchor_instruction_executed":false,"budget_domain":"full-proof spacing + initialized tick bound; low budgets only in admission_probes"},"base_pool":base,"next_bank_pool":next,"cases":cases,"sequences":sequences,"admission_probes":admission,"time_invariance_probes":dynamic_time_probes(timestamp)})
}
