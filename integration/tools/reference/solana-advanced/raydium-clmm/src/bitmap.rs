//! Original bitmap/account adapters. Bit placement is delegated to upstream.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BitmapExtension {
    pub address: String,
    pub positive: Vec<u64>,
    pub negative: Vec<u64>,
}

pub(super) fn original_extension(p: &Pool) -> Option<TickArrayBitmapExtension> {
    p.raydium_clmm.bitmap_extension.as_ref().map(|e| {
        let mut out = TickArrayBitmapExtension::default();
        out.pool_id = pk(&p.pool_id);
        let mut positive = [[0; 8]; 14];
        let mut negative = [[0; 8]; 14];
        for (i, group) in e.positive.chunks_exact(8).enumerate() {
            positive[i].copy_from_slice(group);
        }
        for (i, group) in e.negative.chunks_exact(8).enumerate() {
            negative[i].copy_from_slice(group);
        }
        out.positive_tick_array_bitmap = positive;
        out.negative_tick_array_bitmap = negative;
        out
    })
}
pub(super) fn extension_json(e: &TickArrayBitmapExtension) -> BitmapExtension {
    BitmapExtension {
        address: TickArrayBitmapExtension::key(e.pool_id).to_string(),
        positive: { e.positive_tick_array_bitmap }
            .into_iter()
            .flatten()
            .collect(),
        negative: { e.negative_tick_array_bitmap }
            .into_iter()
            .flatten()
            .collect(),
    }
}
pub(super) fn validate_extension(p: &Pool) -> std::result::Result<(), String> {
    let s = &p.raydium_clmm;
    let e = s.bitmap_extension.as_ref().ok_or("extension missing")?;
    let pool_key = Pubkey::from_str(&p.pool_id).map_err(|_| "pool pubkey")?;
    if e.positive.len() != 112
        || e.negative.len() != 112
        || e.address != TickArrayBitmapExtension::key(pool_key).to_string()
    {
        return Err("extension width / PDA".into());
    }
    if s.tick_spacing == 0 || s.tick_spacing > 1000 {
        return Err("spacing".into());
    }
    let spacing = s.tick_spacing as u16;
    let span = TickArrayState::tick_count(spacing);
    for a in &s.tick_arrays {
        if !TickArrayState::check_is_valid_start_index(a.start_tick_index, spacing) {
            return Err("source tick array domain".into());
        }
        let address = pda(
            &[
                TICK_ARRAY_SEED.as_bytes(),
                pool_key.as_ref(),
                &a.start_tick_index.to_be_bytes(),
            ],
            &pk(PROGRAM),
        )
        .0;
        if a.address != address {
            return Err("array PDA".into());
        }
        let mut previous = None;
        for t in &a.ticks {
            if !(tick_math::MIN_TICK..=tick_math::MAX_TICK).contains(&t.tick)
                || t.tick < a.start_tick_index
                || t.tick >= a.start_tick_index + span
                || t.tick % spacing as i32 != 0
                || previous.is_some_and(|x| x >= t.tick)
                || t.liquidity_gross
                    .parse::<u128>()
                    .ok()
                    .filter(|x| *x > 0)
                    .is_none()
            {
                return Err("initialized tick proof".into());
            }
            previous = Some(t.tick);
        }
    }
    Ok(())
}
pub(super) fn array_initialized(p: &Pool, start: i32) -> std::result::Result<bool, String> {
    let index = start / (60 * p.raydium_clmm.tick_spacing as i32);
    if (-512..512).contains(&index) {
        let bit = (index + 512) as usize;
        return Ok(p.raydium_clmm.bitmap[bit / 64] & (1 << (bit % 64)) != 0);
    }
    original_extension(p)
        .ok_or("extension missing")?
        .check_tick_array_is_initialized(start, p.raydium_clmm.tick_spacing as u16)
        .map(|x| x.0)
        .map_err(|e| e.to_string())
}
pub(super) fn rebuild(p: &mut Pool) {
    let mut original = TickArrayBitmapExtension::default();
    original.initialize(pk(&p.pool_id));
    p.raydium_clmm.bitmap.fill(0);
    for a in &p.raydium_clmm.tick_arrays {
        if a.ticks.is_empty() {
            continue;
        }
        let index = a.start_tick_index / (60 * p.raydium_clmm.tick_spacing as i32);
        if (-512..512).contains(&index) {
            let bit = (index + 512) as usize;
            p.raydium_clmm.bitmap[bit / 64] |= 1u64 << (bit % 64);
        } else {
            original
                .flip_tick_array_bit(a.start_tick_index, p.raydium_clmm.tick_spacing as u16)
                .unwrap();
        }
    }
    p.raydium_clmm.bitmap_extension = Some(extension_json(&original));
}
/// Same original pool identity and raw dependencies; relocate only the proof window/price.
pub fn bitmap_pool(center_index: i32, mode: u32) -> Pool {
    let mut p = base_pool();
    p.protocol = BITMAP_PROFILE.into();
    p.adapter_version = BITMAP_ADAPTER.into();
    let pool_key = pk(&p.pool_id);
    let s = &mut p.raydium_clmm;
    s.collect_fee_on = mode;
    let min_index = TickArrayState::get_array_start_index(tick_math::MIN_TICK, 1) / 60;
    let max_index = TickArrayState::get_array_start_index(tick_math::MAX_TICK, 1) / 60;
    let first =
        (center_index - if center_index >= 0 { 4 } else { 2 }).clamp(min_index, max_index - 6);
    for (a, index) in s.tick_arrays.iter_mut().zip(first..first + 7) {
        a.start_tick_index = index * 60;
        a.address = pda(
            &[
                TICK_ARRAY_SEED.as_bytes(),
                pool_key.as_ref(),
                &a.start_tick_index.to_be_bytes(),
            ],
            &pk(PROGRAM),
        )
        .0;
        for (t, delta) in a.ticks.iter_mut().zip([0, 20, 40]) {
            t.tick = a.start_tick_index + delta;
        }
        a.ticks
            .retain(|t| (tick_math::MIN_TICK..=tick_math::MAX_TICK).contains(&t.tick));
    }
    reset_price(
        &mut p,
        (center_index * 60).clamp(tick_math::MIN_TICK + 1, tick_math::MAX_TICK - 1),
    );
    rebuild(&mut p);
    p
}
pub fn fixture_from_bitmap(p: &Pool) -> Value {
    bounded(p).expect("valid bitmap fixture pool");
    assert!(
        p.protocol == BITMAP_PROFILE || p.protocol == SEED_PROFILE || p.protocol == DYNAMIC_PROFILE
    );
    let mut f = fixture_for(p);
    let raw = original_extension(p).unwrap();
    let address = TickArrayBitmapExtension::key(pk(&p.pool_id)).to_string();
    let bytes = zero_copy_account(&raw);
    assert_eq!(bytes.len(), 1832);
    let a = account(PROGRAM, false, bytes);
    f["rpc_accounts"][&address] = a.clone();
    f["rpc_request_accounts"]
        .as_array_mut()
        .unwrap()
        .push(json!(address));
    f["rpc_response"]["result"]["value"]
        .as_array_mut()
        .unwrap()
        .push(a);
    f["metadata"]["extension_len"] = json!(TickArrayBitmapExtension::LEN);
    f["metadata"]["extension_discriminator"] = json!(TickArrayBitmapExtension::DISCRIMINATOR);
    f["metadata"]["extension_address"] = json!(address);
    f["metadata"]["extension_seed"] = json!(POOL_TICK_ARRAY_BITMAP_SEED);
    f["metadata"]["account_bytes"] = json!("Original packed PoolState/TickArrayState/ObservationState/TickArrayBitmapExtension and Anchor Borsh AmmConfig; classic SPL and synthetic loader/Clock proofs");
    f
}
pub fn bitmap_next_bank_pool() -> Pool {
    let mut p = bitmap_pool(512, 1);
    p.raydium_clmm.tick_arrays.last_mut().unwrap().ticks.clear();
    rebuild(&mut p);
    p
}
fn process_vectors(p: &Pool) -> Value {
    json!([false, true]
        .into_iter()
        .flat_map(|d| [1_000_000, 2_000_000_000u64].map(move |amount| {
            json!({"input":amount.to_string(),"zero_for_one":d,"expected":expected(p, amount, d)})
        }))
        .collect::<Vec<_>>())
}
pub fn bitmap_fixture() -> Value {
    let mut f = fixture_from_bitmap(&bitmap_pool(512, 0));
    f["metadata"]["alternate_negative_base"] = fixture_from_bitmap(&bitmap_pool(-512, 0));
    let p = bitmap_pool(512, 0);
    let after = quote(&p, "1000000", true).unwrap().pool;
    f["metadata"]["alternate_fixture"] = fixture_from_bitmap(&after);
    f["metadata"]["alternate_fixture_source"] =
        json!({"input":"1000000","zero_for_one":true,"native_next_pool":true});
    let bank = bitmap_next_bank_pool();
    f["metadata"]["next_bank_fixture"] = fixture_from_bitmap(&bank);
    f["metadata"]["next_bank_vectors"] = process_vectors(&bank);
    f["metadata"]["next_bank_change"] = json!({"collect_fee_on":[0,1],"cleared_array_start":bank.raydium_clmm.tick_arrays.last().unwrap().start_tick_index,"extension_bit_cleared":true,"initialized_tick_count_after":0,"same_account_window":true});
    let mode_only = bitmap_pool(512, 1);
    f["metadata"]["next_bank_scope_probe"] = json!({
        "input":"30000000000", "zero_for_one":false,
        "before_expected":expected(&p, 30_000_000_000, false),
        "after_expected":expected(&bank, 30_000_000_000, false),
        "mode_only_expected":expected(&mode_only, 30_000_000_000, false)
    });
    f["metadata"]["next_bank_scope_probe_candidates"] = json!(
        [20_000_000_000,25_000_000_000,30_000_000_000,35_000_000_000u64].map(|amount| {
            let before=expected(&p,amount,false); let after=expected(&bank,amount,false);
            json!({"input":amount.to_string(),"before_amount_out":before["amount_out"],"after_amount_out":after["amount_out"],"after_reverted":after.get("reverted").is_some()})
        })
    );
    f["metadata"]["next_bank_collect_fee_on"] = json!(1);
    f["metadata"]["mode_bank_change_vectors"] = json!((0..=1).flat_map(|mode| [false, true].into_iter().flat_map(move |d| [1_000_000,2_000_000_000u64].map(move |amount| {
        let p = bitmap_pool(512, mode);
        json!({"collect_fee_on":mode,"input":amount.to_string(),"zero_for_one":d,"expected":expected(&p, amount, d)})
    }))).collect::<Vec<_>>());
    f
}
/// Each nonzero word comes from the original flip method, never an adapter mapping formula.
pub fn bitmap_vectors() -> Value {
    let mut indices = std::collections::BTreeSet::new();
    for group in 0..14 {
        let lo = 512 * (group + 1);
        for index in [lo, lo + 1, lo + 511, -lo - 1, -lo - 2, -lo - 512] {
            if TickArrayState::check_is_valid_start_index(index * 60, 1) {
                indices.insert(index);
            }
        }
    }
    indices.extend([
        -7394, -7393, -1025, -1024, -513, 512, 1023, 1024, 7392, 7393,
    ]);
    let mut vectors = vec![];
    for index in indices {
        let mut e = TickArrayBitmapExtension::default();
        e.initialize(pk(&base_pool().pool_id));
        e.flip_tick_array_bit(index * 60, 1).unwrap();
        let expected = e.check_tick_array_is_initialized(index * 60, 1).unwrap().0;
        let state = extension_json(&e);
        vectors.push(json!({"index":index,"tick_spacing":1,"start_tick_index":index*60,"positive":state.positive,"negative":state.negative,"expected_initialized":expected}));
    }
    let rejected: Vec<_> = [-7680, -7395, 7394, 7679].into_iter().map(|index| {
        let mut e = TickArrayBitmapExtension::default();
        json!({"index":index,"start_tick_index":index*60,"rejected":e.flip_tick_array_bit(index*60,1).is_err()})
    }).collect();
    json!({"schema_version":1,"revision":REVISION,"source_method":"TickArrayBitmapExtension::flip_tick_array_bit + check_tick_array_is_initialized","tick_spacing":1,"vectors":vectors,"invalid_source_starts":rejected})
}

pub fn generate_bitmap() -> Value {
    let mut cases = vec![];
    let mut vectors = vec![];
    for mode in 0..=2 {
        for d in [false, true] {
            for center in [512, -512, 1024, -1024, 7168, -7168] {
                for amount in [1, 1_000_000, 2_000_000_000, 10_000_000_000, u64::MAX] {
                    let p = bitmap_pool(center, mode);
                    let name = format!("mode{mode}-index{center}-dir{d}-input{amount}");
                    if [512, -512].contains(&center) && [1_000_000, 2_000_000_000].contains(&amount)
                    {
                        vectors.push(name.clone());
                    }
                    add_case(&mut cases, &name, p, amount, d);
                }
            }
            for variant in 0..28 {
                let mut p = bitmap_pool(if d { -512 } else { 512 }, mode);
                let mut amount = 2_000_000_000;
                let current = p
                    .raydium_clmm
                    .tick_arrays
                    .iter()
                    .position(|a| {
                        a.start_tick_index == p.raydium_clmm.tick_current.div_euclid(60) * 60
                    })
                    .unwrap();
                let label = match variant {
                    0 => {
                        set_fee(&mut p, 0, 0, 0);
                        "zero-fee"
                    }
                    1 => {
                        set_fee(&mut p, 999_999, 120_000, 40_000);
                        "ceil-high-fee"
                    }
                    2 => {
                        set_fee(&mut p, 2500, 1_000_000, 0);
                        "protocol-all"
                    }
                    3 => {
                        set_fee(&mut p, 2500, 0, 1_000_000);
                        "fund-all"
                    }
                    4 => {
                        set_fee(&mut p, 2500, 0, 0);
                        "lp-only"
                    }
                    5 => {
                        p.raydium_clmm.tick_arrays[current].ticks.clear();
                        rebuild(&mut p);
                        "empty-current-bitmap"
                    }
                    6 => {
                        for a in &mut p.raydium_clmm.tick_arrays {
                            a.ticks.clear();
                        }
                        rebuild(&mut p);
                        "empty-bitmap-exhaustion"
                    }
                    7 => {
                        p.raydium_clmm.tick_arrays[if d { current - 1 } else { current + 1 }]
                            .ticks
                            .clear();
                        rebuild(&mut p);
                        "empty-bitmap-gap"
                    }
                    8 => {
                        p.raydium_clmm.bitmap_extension = None;
                        "missing-extension-proof"
                    }
                    9 => {
                        p.raydium_clmm
                            .bitmap_extension
                            .as_mut()
                            .unwrap()
                            .positive
                            .pop();
                        "positive-width"
                    }
                    10 => {
                        p.raydium_clmm
                            .bitmap_extension
                            .as_mut()
                            .unwrap()
                            .negative
                            .push(0);
                        "negative-width"
                    }
                    11 => {
                        p.raydium_clmm.bitmap_extension.as_mut().unwrap().address =
                            key("wrong extension").to_string();
                        "extension-PDA"
                    }
                    12 => {
                        p.raydium_clmm.raw_vault0 = u64::MAX;
                        p.raydium_clmm.raw_vault1 = u64::MAX;
                        "raw-input-overflow"
                    }
                    13 => {
                        p.raydium_clmm.raw_vault0 = 0;
                        p.raydium_clmm.raw_vault1 = 0;
                        "raw-output-underflow"
                    }
                    14 => {
                        p.raydium_clmm.protocol_fees0 = u64::MAX;
                        p.raydium_clmm.protocol_fees1 = u64::MAX;
                        "protocol-counter-overflow"
                    }
                    15 => {
                        p.raydium_clmm.fund_fees0 = u64::MAX;
                        p.raydium_clmm.fund_fees1 = u64::MAX;
                        "fund-counter-overflow"
                    }
                    16 => {
                        p.raydium_clmm.fee_growth_global0 = (u128::MAX - 2).to_string();
                        p.raydium_clmm.fee_growth_global1 = (u128::MAX - 2).to_string();
                        "growth-wrap"
                    }
                    17 => {
                        p.raydium_clmm.liquidity = u128::MAX.to_string();
                        "u128-max-liquidity"
                    }
                    18 => {
                        amount = u64::MAX;
                        "u64-max-amount"
                    }
                    19 => {
                        p.raydium_clmm.max_steps = 1;
                        // False-direction 2b stays inside one native compute step.
                        // Use 30b so parity tests genuinely require multiple steps;
                        // the service keeps its actual runtime budget semantics.
                        if !d {
                            amount = 30_000_000_000;
                        }
                        "conservative-max-steps"
                    }
                    20 => {
                        p = bitmap_pool(if d { -7394 } else { 7393 }, mode);
                        "ticklimits-exhaustion"
                    }
                    21 => {
                        amount = 1_000_000_000_000_000;
                        "partial-window-exhaustion"
                    }
                    22 => {
                        p.raydium_clmm.observation_initialized = true;
                        p.raydium_clmm.observation_index = 99;
                        let s = &mut p.raydium_clmm;
                        s.observations[99].block_timestamp = s.bank_unix_timestamp as u32 - 30;
                        s.observations[99].tick_cumulative = i64::MAX - 5;
                        "observation-wrap"
                    }
                    23 => {
                        for a in &mut p.raydium_clmm.tick_arrays {
                            for t in &mut a.ticks {
                                t.liquidity_net = "100000000".into();
                            }
                        }
                        "cross-liquidity-delta"
                    }
                    24 => {
                        // Keep genuine initialized bits immediately beyond the supplied proof window.
                        let mut e = original_extension(&p).unwrap();
                        let index = if d { -516 } else { 516 };
                        e.flip_tick_array_bit(index * 60, 1).unwrap();
                        p.raydium_clmm.bitmap_extension = Some(extension_json(&e));
                        amount = 1_000_000_000_000;
                        "selected-missing-proof-array"
                    }
                    25 => {
                        // Preserve logical high bits, including bits outside the actual tick domain.
                        let e = p.raydium_clmm.bitmap_extension.as_mut().unwrap();
                        e.positive[111] = u64::MAX;
                        e.negative[111] = u64::MAX;
                        "retained-outside-domain-bits"
                    }
                    26 => {
                        p.raydium_clmm.tick_arrays[current].ticks.clear();
                        "bitmap-count-mismatch"
                    }
                    _ => {
                        amount = 0;
                        "zero-amount"
                    }
                };
                add_case(
                    &mut cases,
                    &format!("mode{mode}-dir{d}-{label}"),
                    p,
                    amount,
                    d,
                );
            }
        }
    }
    for d in [false, true] {
        for amount in [1_000_000, 2_000_000_000u64] {
            add_case(
                &mut cases,
                &format!("next-bank-mode1-cleared-extension-dir{d}-input{amount}"),
                bitmap_next_bank_pool(),
                amount,
                d,
            );
        }
    }
    for mode in 0..=2 {
        for d in [false, true] {
            let before = bitmap_pool(512, mode);
            let mut after = before.clone();
            after
                .raydium_clmm
                .tick_arrays
                .last_mut()
                .unwrap()
                .ticks
                .clear();
            rebuild(&mut after);
            add_case(
                &mut cases,
                &format!("scope-probe-mode{mode}-dir{d}-before"),
                before,
                30_000_000_000,
                d,
            );
            add_case(
                &mut cases,
                &format!("scope-probe-mode{mode}-dir{d}-cleared"),
                after,
                30_000_000_000,
                d,
            );
        }
    }
    let mut sequences = vec![];
    for mode in 0..=2 {
        for directions in [[true, true], [false, false], [true, false], [false, true]] {
            let p = bitmap_pool(if directions[0] { 512 } else { -512 }, mode);
            let mut current = p.clone();
            let mut steps = vec![];
            for (d, amount) in directions.into_iter().zip([1_000_000, 2_000_000_000u64]) {
                let q =
                    quote(&current, &amount.to_string(), d).expect("native sequence must succeed");
                let result = json!({"amount_out":q.amount_out.to_string(),"next_pool":q.pool});
                current = q.pool;
                steps.push(json!({"input":amount.to_string(),"zero_for_one":d,"expected":result}));
            }
            sequences.push(json!({"name":format!("mode{mode}-sequence-{}-{}",directions[0],directions[1]),"pool":p,"steps":steps,"expected_final_pool":current}));
        }
    }
    json!({"schema_version":1,"reference":{"revision":REVISION,"profile":BITMAP_PROFILE,"adapter_version":BITMAP_ADAPTER,"source":"unchanged native swap_internal with actual optional extension AccountInfo; original AccountLoad Ref/RefMut","sbf_transaction_executed":false,"business_math_imported":false},"base_pool":bitmap_pool(512,0),"negative_base_pool":bitmap_pool(-512,0),"next_bank_pool":bitmap_next_bank_pool(),"independent_vectors":vectors,"cases":cases,"sequences":sequences})
}
