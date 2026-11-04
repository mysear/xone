//! Native oracle: original Raydium program, with only typed-state/account adapters.
use anchor_lang::{prelude::*, AccountSerialize, Discriminator};
use base64::{engine::general_purpose::STANDARD, Engine};
use raydium_clmm::{instructions::swap::swap_internal, libraries::tick_math, states::*};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::VecDeque, str::FromStr, sync::Mutex};

pub const REVISION: &str = "6d7a0f82d2f293c553c69aaf943377367c6c4cc6";
pub const PROGRAM: &str = "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK";
pub const PROFILE: &str = "raydium-clmm-rewards";
pub const ADAPTER: &str = "raydium-clmm/6d7a0f82-spl-rewards-1";
pub const TOKEN_FEE_PROFILE: &str = "raydium-clmm-token-fee";
pub const TOKEN_FEE_ADAPTER: &str = "raydium-clmm/6d7a0f82-spl-token-fee-1";
mod dynamic;
pub use dynamic::*;
mod seed;
pub use seed::*;
mod bitmap;
pub use bitmap::*;
pub const BITMAP_PROFILE: &str = "raydium-clmm-bitmap";
pub const BITMAP_ADAPTER: &str = "raydium-clmm/6d7a0f82-spl-bitmap-1";
pub const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const LOADER: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
pub const IMMUTABLE: &str = "BPFLoader2111111111111111111111111111111111";
pub const CLOCK: &str = "SysvarC1ock11111111111111111111111111111111";
pub const PROGRAM_CODE: &[u8] =
    b"XONE synthetic Raydium CLMM proof; NOT deployed bytecode; 6d7a0f82";
pub const TOKEN_CODE: &[u8] =
    b"XONE synthetic classic SPL Token proof fixture; NOT deployed token bytecode";
pub fn sha256(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn key(label: &str) -> Pubkey {
    Pubkey::new_from_array(Sha256::digest(label.as_bytes()).into())
}
fn pk(s: &str) -> Pubkey {
    Pubkey::from_str(s).expect("canonical public key")
}
fn pda(seeds: &[&[u8]], program: &Pubkey) -> (String, u8) {
    let (p, b) = Pubkey::find_program_address(seeds, program);
    (p.to_string(), b)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Reward {
    pub reward_state: u32, pub open_time:u64, pub end_time:u64, pub last_update_time:u64,
    pub emissions_per_second_x64:String, pub reward_total_emitted:u64, pub reward_claimed:u64,
    pub token_mint:String, pub token_vault:String, pub authority:String, pub reward_growth_global_x64:String,
}
impl Reward {
    fn original(&self) -> RewardInfo { RewardInfo {
        reward_state:self.reward_state as u8, open_time:self.open_time, end_time:self.end_time, last_update_time:self.last_update_time,
        emissions_per_second_x64:self.emissions_per_second_x64.parse().unwrap(), reward_total_emitted:self.reward_total_emitted,
        reward_claimed:self.reward_claimed, token_mint:pk(&self.token_mint), token_vault:pk(&self.token_vault), authority:pk(&self.authority),
        reward_growth_global_x64:self.reward_growth_global_x64.parse().unwrap(),
    } }
    fn from_original(r:RewardInfo) -> Self { Self {
        reward_state:r.reward_state as u32, open_time:r.open_time, end_time:r.end_time, last_update_time:r.last_update_time,
        emissions_per_second_x64:{r.emissions_per_second_x64}.to_string(),reward_total_emitted:r.reward_total_emitted,
        reward_claimed:r.reward_claimed,token_mint:r.token_mint.to_string(),token_vault:r.token_vault.to_string(),authority:r.authority.to_string(),
        reward_growth_global_x64:{r.reward_growth_global_x64}.to_string(),
    } }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Tick {
    #[serde(default,skip_serializing_if="Vec::is_empty")]
    pub reward_growths_outside:Vec<String>,
    pub tick: i32,
    pub liquidity_gross: String,
    pub liquidity_net: String,
    pub fee_growth_outside0: String,
    pub fee_growth_outside1: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Array {
    pub address: String,
    pub start_tick_index: i32,
    pub recent_epoch: u64,
    pub ticks: Vec<Tick>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ObservationJSON {
    pub block_timestamp: u32,
    pub tick_cumulative: i64,
}
fn is_zero(value: &u32) -> bool {
    *value == 0
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct State {
    #[serde(default,skip_serializing_if="Vec::is_empty")]
    pub reward_infos:Vec<Reward>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub seed_index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamic_fee: Option<RaydiumCLMMDynamicFee>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bitmap_extension: Option<BitmapExtension>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub collect_fee_on: u32,
    pub program_id: String,
    pub programdata: String,
    pub program_code_sha256: String,
    pub deployed_slot: u64,
    pub token_program_code_sha256: String,
    pub amm_config: String,
    pub pool_bump: u32,
    pub observation: String,
    pub vault0: String,
    pub vault1: String,
    pub raw_vault0: u64,
    pub raw_vault1: u64,
    pub mint0_decimals: u32,
    pub mint1_decimals: u32,
    pub bank_slot: u64,
    pub bank_unix_timestamp: u64,
    pub bank_epoch: u64,
    pub trade_fee_rate: u32,
    pub protocol_fee_rate: u32,
    pub fund_fee_rate: u32,
    pub tick_spacing: u32,
    pub sqrt_price_x64: String,
    pub tick_current: i32,
    pub liquidity: String,
    pub fee_growth_global0: String,
    pub fee_growth_global1: String,
    pub protocol_fees0: u64,
    pub protocol_fees1: u64,
    pub fund_fees0: u64,
    pub fund_fees1: u64,
    pub open_time: u64,
    pub recent_epoch: u64,
    pub bitmap: Vec<u64>,
    pub tick_arrays: Vec<Array>,
    pub max_steps: u32,
    pub observation_initialized: bool,
    pub observation_index: u32,
    pub observation_recent_epoch: u64,
    pub observations: Vec<ObservationJSON>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Pool {
    pub chain_key: String,
    pub pool_id: String,
    pub protocol: String,
    pub adapter_version: String,
    pub token0: String,
    pub token1: String,
    pub reserve0: String,
    pub reserve1: String,
    pub fee_numerator: u64,
    pub fee_denominator: u64,
    pub token_behavior: String,
    pub raydium_clmm: State,
}
pub struct Quote {
    pub amount_out: u64,
    pub pool: Pool,
}

// All native quotes are serialized around process-global Solana syscall stubs.
static NATIVE_LOCK: Mutex<()> = Mutex::new(());
struct ClockStub(Clock);
impl solana_sysvar::program_stubs::SyscallStubs for ClockStub {
    fn sol_log(&self, _message: &str) {}
    fn sol_get_clock_sysvar(&self, addr: *mut u8) -> u64 {
        // Runtime syscall ABI writes an initialized native Clock at the supplied pointer.
        unsafe {
            std::ptr::write_unaligned(addr.cast::<Clock>(), self.0.clone());
        }
        0
    }
}
struct RestoreStubs(Option<Box<dyn solana_sysvar::program_stubs::SyscallStubs>>);
impl Drop for RestoreStubs {
    fn drop(&mut self) {
        if let Some(old) = self.0.take() {
            solana_sysvar::program_stubs::set_syscall_stubs(old);
        }
    }
}
fn clock(s: &State) -> Clock {
    Clock {
        slot: s.bank_slot,
        epoch_start_timestamp: 0,
        epoch: s.bank_epoch,
        leader_schedule_epoch: s.bank_epoch,
        unix_timestamp: s.bank_unix_timestamp as i64,
    }
}
fn install(s: &State) -> RestoreStubs {
    RestoreStubs(Some(solana_sysvar::program_stubs::set_syscall_stubs(
        Box::new(ClockStub(clock(s))),
    )))
}

pub fn base_pool() -> Pool {
    let program = pk(PROGRAM);
    let mut mints = [key("rayclmm mint A"), key("rayclmm mint B")];
    mints.sort_by_key(|x| x.to_bytes());
    let (config, _) = pda(&[AMM_CONFIG_SEED.as_bytes(), &0u16.to_be_bytes()], &program);
    let (pool, bump) = pda(
        &[
            POOL_SEED.as_bytes(),
            pk(&config).as_ref(),
            mints[0].as_ref(),
            mints[1].as_ref(),
        ],
        &program,
    );
    let (v0, _) = pda(
        &[
            POOL_VAULT_SEED.as_bytes(),
            pk(&pool).as_ref(),
            mints[0].as_ref(),
        ],
        &program,
    );
    let (v1, _) = pda(
        &[
            POOL_VAULT_SEED.as_bytes(),
            pk(&pool).as_ref(),
            mints[1].as_ref(),
        ],
        &program,
    );
    let (obs, _) = pda(&[OBSERVATION_SEED.as_bytes(), pk(&pool).as_ref()], &program);
    let (pd, _) = pda(&[program.as_ref()], &pk(LOADER));
    let mut arrays = vec![];
    let mut bitmap = vec![0u64; 16];
    for start in [-180i32, -120, -60, 0, 60, 120, 180] {
        let ticks = (0..60)
            .step_by(20)
            .map(|i| Tick {
                reward_growths_outside: Vec::new(),
                tick: start + i,
                liquidity_gross: "1000000000000".into(),
                liquidity_net: "0".into(),
                fee_growth_outside0: "0".into(),
                fee_growth_outside1: "0".into(),
            })
            .collect();
        let (address, _) = pda(
            &[
                TICK_ARRAY_SEED.as_bytes(),
                pk(&pool).as_ref(),
                &start.to_be_bytes(),
            ],
            &program,
        );
        arrays.push(Array {
            address,
            start_tick_index: start,
            recent_epoch: 986,
            ticks,
        });
        let bit = (start / 60 + 512) as usize;
        bitmap[bit / 64] |= 1u64 << (bit % 64);
    }
    Pool {
        chain_key: "solana:localnet".into(),
        pool_id: pool,
        protocol: PROFILE.into(),
        adapter_version: ADAPTER.into(),
        token0: mints[0].to_string(),
        token1: mints[1].to_string(),
        reserve0: "0".into(),
        reserve1: "0".into(),
        fee_numerator: 2500,
        fee_denominator: 1_000_000,
        token_behavior: "standard".into(),
        raydium_clmm: State {
            reward_infos:Vec::new(),
            seed_index: 0,
            dynamic_fee: None,
            bitmap_extension: None,
            collect_fee_on: 0,
            program_id: PROGRAM.into(),
            programdata: pd,
            program_code_sha256: sha256(PROGRAM_CODE),
            deployed_slot: 200,
            token_program_code_sha256: sha256(TOKEN_CODE),
            amm_config: config,
            pool_bump: bump as u32,
            observation: obs,
            vault0: v0,
            vault1: v1,
            raw_vault0: 1_000_000_000_000_000,
            raw_vault1: 1_000_000_000_000_000,
            mint0_decimals: 9,
            mint1_decimals: 6,
            bank_slot: 123456,
            bank_unix_timestamp: 1792972800,
            bank_epoch: 987,
            trade_fee_rate: 2500,
            protocol_fee_rate: 120000,
            fund_fee_rate: 40000,
            tick_spacing: 1,
            sqrt_price_x64: tick_math::get_sqrt_price_at_tick(10).unwrap().to_string(),
            tick_current: 10,
            liquidity: "1000000000000".into(),
            fee_growth_global0: "100".into(),
            fee_growth_global1: "200".into(),
            protocol_fees0: 101,
            protocol_fees1: 103,
            fund_fees0: 211,
            fund_fees1: 223,
            open_time: 1,
            recent_epoch: 986,
            bitmap,
            tick_arrays: arrays,
            max_steps: 8192,
            observation_initialized: false,
            observation_index: 0,
            observation_recent_epoch: 986,
            observations: vec![
                ObservationJSON {
                    block_timestamp: 0,
                    tick_cumulative: 0
                };
                100
            ],
        },
    }
}
fn original_states(p: &Pool) -> (AmmConfig, PoolState, Vec<TickArrayState>, ObservationState) {
    let s = &p.raydium_clmm;
    let config = AmmConfig {
        trade_fee_rate: s.trade_fee_rate,
        protocol_fee_rate: s.protocol_fee_rate,
        fund_fee_rate: s.fund_fee_rate,
        tick_spacing: s.tick_spacing as u16,
        ..AmmConfig::default()
    };
    let mut state = PoolState::default();
    state.set_fee_on(s.collect_fee_on as u8).unwrap();
    state.set_seed_index(u16::try_from(s.seed_index).expect("validated u16 seed"));
    state.dynamic_fee_info = s
        .dynamic_fee
        .as_ref()
        .map(RaydiumCLMMDynamicFee::original)
        .unwrap_or_default();
    state.bump = [s.pool_bump as u8];
    state.amm_config = pk(&s.amm_config);
    state.token_mint_0 = pk(&p.token0);
    state.token_mint_1 = pk(&p.token1);
    state.token_vault_0 = pk(&s.vault0);
    state.token_vault_1 = pk(&s.vault1);
    state.observation_key = pk(&s.observation);
    state.mint_decimals_0 = s.mint0_decimals as u8;
    state.mint_decimals_1 = s.mint1_decimals as u8;
    state.tick_spacing = s.tick_spacing as u16;
    state.liquidity = s.liquidity.parse().unwrap();
    state.sqrt_price_x64 = s.sqrt_price_x64.parse().unwrap();
    state.tick_current = s.tick_current;
    state.fee_growth_global_0_x64 = s.fee_growth_global0.parse().unwrap();
    state.fee_growth_global_1_x64 = s.fee_growth_global1.parse().unwrap();
    state.protocol_fees_token_0 = s.protocol_fees0;
    state.protocol_fees_token_1 = s.protocol_fees1;
    state.fund_fees_token_0 = s.fund_fees0;
    state.fund_fees_token_1 = s.fund_fees1;
    if !s.reward_infos.is_empty() { state.reward_infos = s.reward_infos.iter().map(Reward::original).collect::<Vec<_>>().try_into().unwrap(); }
    state.open_time = s.open_time;
    state.recent_epoch = s.recent_epoch;
    state.tick_array_bitmap = s.bitmap.clone().try_into().unwrap();
    assert_eq!(state.key().to_string(), p.pool_id);
    let arrays = s
        .tick_arrays
        .iter()
        .map(|a| {
            let mut out = TickArrayState::default();
            out.pool_id = pk(&p.pool_id);
            out.start_tick_index = a.start_tick_index;
            out.recent_epoch = a.recent_epoch;
            out.initialized_tick_count = a.ticks.len() as u8;
            for t in &a.ticks {
                let index = ((t.tick - a.start_tick_index) / s.tick_spacing as i32) as usize;
                out.ticks[index] = TickState {
                    tick: t.tick,
                    liquidity_gross: t.liquidity_gross.parse().unwrap(),
                    liquidity_net: t.liquidity_net.parse().unwrap(),
                    fee_growth_outside_0_x64: t.fee_growth_outside0.parse().unwrap(),
                    fee_growth_outside_1_x64: t.fee_growth_outside1.parse().unwrap(),
                    reward_growths_outside_x64: if t.reward_growths_outside.is_empty() { [0;3] } else {t.reward_growths_outside.iter().map(|v|v.parse().unwrap()).collect::<Vec<_>>().try_into().unwrap()},
                    ..TickState::default()
                };
            }
            assert_eq!(out.key().to_string(), a.address);
            out
        })
        .collect();
    let mut obs = ObservationState::default();
    obs.initialized = s.observation_initialized;
    obs.pool_id = pk(&p.pool_id);
    obs.recent_epoch = s.observation_recent_epoch;
    obs.observation_index = s.observation_index as u16;
    for (i, o) in s.observations.iter().enumerate() {
        obs.observations[i].block_timestamp = o.block_timestamp;
        obs.observations[i].tick_cumulative = o.tick_cumulative;
    }
    (config, state, arrays, obs)
}
fn extract(p: &Pool, state: PoolState, arrays: Vec<TickArrayState>, obs: ObservationState) -> Pool {
    let mut out = p.clone();
    let s = &mut out.raydium_clmm;
    if s.dynamic_fee.is_some() {
        s.dynamic_fee = Some(RaydiumCLMMDynamicFee::from_original(state.dynamic_fee_info));
    }
    s.sqrt_price_x64 = { state.sqrt_price_x64 }.to_string();
    if !s.reward_infos.is_empty() { s.reward_infos = {state.reward_infos}.into_iter().map(Reward::from_original).collect(); }
    s.tick_current = state.tick_current;
    s.liquidity = { state.liquidity }.to_string();
    s.fee_growth_global0 = { state.fee_growth_global_0_x64 }.to_string();
    s.fee_growth_global1 = { state.fee_growth_global_1_x64 }.to_string();
    s.protocol_fees0 = state.protocol_fees_token_0;
    s.protocol_fees1 = state.protocol_fees_token_1;
    s.fund_fees0 = state.fund_fees_token_0;
    s.fund_fees1 = state.fund_fees_token_1;
    s.recent_epoch = state.recent_epoch;
    s.bitmap = { state.tick_array_bitmap }.to_vec();
    for (a, raw) in s.tick_arrays.iter_mut().zip(arrays) {
        a.recent_epoch = raw.recent_epoch;
        a.ticks = raw
            .ticks
            .into_iter()
            .filter(|t| t.liquidity_gross != 0)
            .map(|t| Tick {
                reward_growths_outside: if s.reward_infos.is_empty() {Vec::new()} else {{t.reward_growths_outside_x64}.into_iter().map(|v|v.to_string()).collect()},
                tick: t.tick,
                liquidity_gross: { t.liquidity_gross }.to_string(),
                liquidity_net: { t.liquidity_net }.to_string(),
                fee_growth_outside0: { t.fee_growth_outside_0_x64 }.to_string(),
                fee_growth_outside1: { t.fee_growth_outside_1_x64 }.to_string(),
            })
            .collect();
    }
    s.observation_initialized = obs.initialized;
    s.observation_index = obs.observation_index as u32;
    s.observation_recent_epoch = obs.recent_epoch;
    s.observations = obs
        .observations
        .into_iter()
        .map(|o| ObservationJSON {
            block_timestamp: o.block_timestamp,
            tick_cumulative: o.tick_cumulative,
        })
        .collect();
    out
}
fn bounded(p: &Pool) -> std::result::Result<(), String> {
    let s = &p.raydium_clmm;
    if p.protocol != DYNAMIC_PROFILE && s.dynamic_fee.is_some() {
        return Err("dynamic fee in old profile".into());
    }
    match (
        p.protocol.as_str(),
        p.adapter_version.as_str(),
        s.collect_fee_on,
    ) {
        (PROFILE, ADAPTER, 0) | (TOKEN_FEE_PROFILE, TOKEN_FEE_ADAPTER, 1 | 2)
            if s.seed_index == 0 && s.bitmap_extension.is_none() => {}
        (BITMAP_PROFILE, BITMAP_ADAPTER, 0..=2)
            if s.seed_index == 0 && s.bitmap_extension.is_some() =>
        {
            validate_extension(p)?;
        }
        (DYNAMIC_PROFILE, DYNAMIC_ADAPTER, 0..=2) => {
            validate_seed_identity(p)?;
            validate_extension(p)?;
            validate_dynamic(p)?;
        }
        (SEED_PROFILE, SEED_ADAPTER, 0..=2) => {
            validate_seed_identity(p)?;
            validate_extension(p)?;
        }
        _ => return Err("bounded profile / adapter / collect_fee_on / seed_index".into()),
    }
    if s.bank_unix_timestamp > u32::MAX as u64 || s.bank_unix_timestamp <= s.open_time {
        return Err("bounded bank time / entrypoint open_time".into());
    }
    if s.trade_fee_rate >= 1_000_000
        || s.protocol_fee_rate as u64 + s.fund_fee_rate as u64 > 1_000_000
        || s.tick_spacing == 0
        || s.tick_spacing > 1000
        || s.observation_index >= 100
        || s.observations.len() != 100
        || s.bitmap.len() != 16
        || !(3..=7).contains(&s.tick_arrays.len())
        || s.max_steps == 0
        || s.max_steps > 8192
    {
        return Err("bounded static parameters".into());
    }
    // Static fee/no orders has at most one compute step per initialized tick and
    // a final partial step. This conservative preflight is stricter than runtime.
    if s.dynamic_fee.is_none()
        && s.tick_arrays
            .iter()
            .map(|a| a.ticks.len() as u32)
            .sum::<u32>()
            + 1
            > s.max_steps
    {
        return Err("conservative max_steps admission".into());
    }
    if s.dynamic_fee.is_some() {
        dynamic_admission(p)?;
    }
    let span = s.tick_spacing as i32 * 60;
    for (i, a) in s.tick_arrays.iter().enumerate() {
        if a.start_tick_index % span != 0
            || !(if s.bitmap_extension.is_some() {
                -7680..7680
            } else {
                -512..512
            })
            .contains(&(a.start_tick_index / span))
            || (i > 0 && a.start_tick_index != s.tick_arrays[i - 1].start_tick_index + span)
        {
            return Err("base array window".into());
        }
        let initialized = array_initialized(p, a.start_tick_index)?;
        if initialized != !a.ticks.is_empty() {
            return Err("bitmap proof".into());
        }
    }
    Ok(())
}
pub fn quote(p: &Pool, input: &str, zero_for_one: bool) -> std::result::Result<Quote, String> {
    bounded(p)?;
    let amount: u64 = input.parse::<u64>().map_err(|e| e.to_string())?;
    let _lock = NATIVE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _stubs = install(&p.raydium_clmm);
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> std::result::Result<Quote, String> {
            let (config, state, arrays, obs) = original_states(p);
            let state = RefCell::new(state);
            let obs = RefCell::new(obs);
            let arrays: Vec<_> = arrays.into_iter().map(RefCell::new).collect();
            let mut ordered: Vec<_> = arrays.iter().collect();
            if zero_for_one {
                ordered.reverse();
            }
            let mut queue: VecDeque<_> = ordered.iter().map(|a| a.borrow_mut()).collect();
            let limit = if zero_for_one {
                tick_math::MIN_SQRT_PRICE_X64 + 1
            } else {
                tick_math::MAX_SQRT_PRICE_X64 - 1
            };
            // The unchanged signature takes Option<&AccountInfo>; upstream AccountLoad
            // borrows the original extension as Ref/RefMut. Preserve full bytes.
            let extension = original_extension(p);
            let extension_key = extension
                .as_ref()
                .map(|_| TickArrayBitmapExtension::key(pk(&p.pool_id)));
            let mut extension_data = extension.as_ref().map(zero_copy_account);
            let extension_owner = pk(PROGRAM);
            let mut extension_lamports = 10_000_000_000;
            let extension_info = extension_data.as_mut().map(|data| {
                AccountInfo::new(
                    extension_key.as_ref().unwrap(),
                    false,
                    true,
                    &mut extension_lamports,
                    data.as_mut_slice(),
                    &extension_owner,
                    false,
                )
            });
            // Actual original swap mutates RefMut pool, arrays and observations,
            // reads the supplied original extension, and executes Clock::get().
            let result = swap_internal(
                &config,
                &mut state.borrow_mut(),
                &mut queue,
                &mut obs.borrow_mut(),
                extension_info.as_ref(),
                amount,
                limit,
                zero_for_one,
                true,
                raydium_clmm::states::oracle::block_timestamp(),
            )
            .map_err(|e| e.to_string())?;
            drop(queue);
            let (used, output) = if zero_for_one {
                (result.amount_0, result.amount_1)
            } else {
                (result.amount_1, result.amount_0)
            };
            if used != amount || used == 0 || output == 0 {
                return Err("entrypoint requires nonzero output and full input".into());
            }
            let mut next = extract(
                p,
                state.into_inner(),
                arrays.into_iter().map(RefCell::into_inner).collect(),
                obs.into_inner(),
            );
            if let Some(info) = extension_info.as_ref() {
                let data = info.try_borrow_data().map_err(|e| e.to_string())?;
                let raw: TickArrayBitmapExtension = bytemuck::pod_read_unaligned(&data[8..]);
                next.raydium_clmm.bitmap_extension = Some(extension_json(&raw));
            }
            let s = &mut next.raydium_clmm;
            let (input_raw, output_raw) = if zero_for_one {
                (&mut s.raw_vault0, &mut s.raw_vault1)
            } else {
                (&mut s.raw_vault1, &mut s.raw_vault0)
            };
            *input_raw = input_raw
                .checked_add(used)
                .ok_or("SPL input vault overflow")?;
            *output_raw = output_raw
                .checked_sub(output)
                .ok_or("SPL output vault insufficient funds")?;
            Ok(Quote {
                amount_out: output,
                pool: next,
            })
        },
    ))
    .map_err(|_| "original program panic / invalid typed state".to_string())?
}
fn expected(p: &Pool, input: u64, d: bool) -> Value {
    match quote(p, &input.to_string(), d) {
        Ok(q) => json!({"amount_out":q.amount_out.to_string(),"next_pool":q.pool}),
        Err(_) => json!({"reverted":true}),
    }
}
fn account(owner: &str, executable: bool, data: Vec<u8>) -> Value {
    let b64 = STANDARD.encode(&data);
    json!({"owner":owner,"executable":executable,"lamports":10_000_000_000u64,"rentEpoch":0,"data":[b64,"base64"],"data_base64":b64,"space":data.len()})
}
fn zero_copy_account<T: bytemuck::Pod + Discriminator>(s: &T) -> Vec<u8> {
    let mut out = T::DISCRIMINATOR.to_vec();
    out.extend_from_slice(bytemuck::bytes_of(s));
    out
}
fn mint_bytes(decimals: u32) -> Vec<u8> {
    let mut out = vec![0; 82];
    out[36..44].copy_from_slice(&10_000_000_000_000_000u64.to_le_bytes());
    out[44] = decimals as u8;
    out[45] = 1;
    out
}
fn vault_bytes(mint: &str, owner: &str, amount: u64) -> Vec<u8> {
    let mut out = vec![0; 165];
    out[..32].copy_from_slice(pk(mint).as_ref());
    out[32..64].copy_from_slice(pk(owner).as_ref());
    out[64..72].copy_from_slice(&amount.to_le_bytes());
    out[108] = 1;
    out
}
pub fn fixture() -> Value {
    fixture_for(&base_pool())
}
pub fn fixture_for(p: &Pool) -> Value {
    let s = &p.raydium_clmm;
    let (mut config, state, arrays, obs) = original_states(&p);
    let (_, cb) = pda(
        &[AMM_CONFIG_SEED.as_bytes(), &0u16.to_be_bytes()],
        &pk(PROGRAM),
    );
    config.bump = cb;
    config.index = 0;
    let mut config_data = vec![];
    config.try_serialize(&mut config_data).unwrap();
    let mut rpc = serde_json::Map::new();
    rpc.insert(
        p.pool_id.clone(),
        account(PROGRAM, false, zero_copy_account(&state)),
    );
    rpc.insert(s.amm_config.clone(), account(PROGRAM, false, config_data));
    rpc.insert(
        s.observation.clone(),
        account(PROGRAM, false, zero_copy_account(&obs)),
    );
    rpc.insert(
        p.token0.clone(),
        account(TOKEN, false, mint_bytes(s.mint0_decimals)),
    );
    rpc.insert(
        p.token1.clone(),
        account(TOKEN, false, mint_bytes(s.mint1_decimals)),
    );
    rpc.insert(
        s.vault0.clone(),
        account(
            TOKEN,
            false,
            vault_bytes(&p.token0, &p.pool_id, s.raw_vault0),
        ),
    );
    rpc.insert(
        s.vault1.clone(),
        account(
            TOKEN,
            false,
            vault_bytes(&p.token1, &p.pool_id, s.raw_vault1),
        ),
    );
    for (a, raw) in s.tick_arrays.iter().zip(&arrays) {
        rpc.insert(
            a.address.clone(),
            account(PROGRAM, false, zero_copy_account(raw)),
        );
    }
    let mut program = 2u32.to_le_bytes().to_vec();
    program.extend_from_slice(pk(&s.programdata).as_ref());
    rpc.insert(PROGRAM.into(), account(LOADER, true, program));
    let mut pd = vec![0; 45];
    pd[..4].copy_from_slice(&3u32.to_le_bytes());
    pd[4..12].copy_from_slice(&s.deployed_slot.to_le_bytes());
    pd.extend_from_slice(PROGRAM_CODE);
    rpc.insert(s.programdata.clone(), account(LOADER, false, pd));
    rpc.insert(TOKEN.into(), account(IMMUTABLE, true, TOKEN_CODE.to_vec()));
    let c = clock(s);
    let mut clock_data = vec![];
    clock_data.extend_from_slice(&c.slot.to_le_bytes());
    clock_data.extend_from_slice(&c.epoch_start_timestamp.to_le_bytes());
    clock_data.extend_from_slice(&c.epoch.to_le_bytes());
    clock_data.extend_from_slice(&c.leader_schedule_epoch.to_le_bytes());
    clock_data.extend_from_slice(&c.unix_timestamp.to_le_bytes());
    rpc.insert(
        CLOCK.into(),
        account(
            "Sysvar1111111111111111111111111111111111111",
            false,
            clock_data,
        ),
    );
    let starts: Vec<_> = s.tick_arrays.iter().map(|a| a.start_tick_index).collect();
    let mut request = vec![
        p.pool_id.clone(),
        s.vault0.clone(),
        s.vault1.clone(),
        s.amm_config.clone(),
        p.token0.clone(),
        p.token1.clone(),
        s.observation.clone(),
        PROGRAM.into(),
        s.programdata.clone(),
        TOKEN.into(),
        CLOCK.into(),
    ];
    request.extend(s.tick_arrays.iter().map(|a| a.address.clone()));
    let values: Vec<_> = request.iter().map(|k| rpc[k].clone()).collect();
    json!({"schema_version":1,"pool":p,"pool_config":{
        "id":p.pool_id,"protocol":p.protocol,"program_id":PROGRAM,"token0":p.token0,"token1":p.token1,"fee_numerator":s.trade_fee_rate,"fee_denominator":1_000_000,
        "accounts":[p.pool_id,s.vault0,s.vault1,s.amm_config,p.token0,p.token1,s.observation],
        "raydium_clmm":{"expected_programdata":s.programdata,"expected_program_code_sha256":s.program_code_sha256,"expected_deployed_slot":s.deployed_slot,"expected_token_program_code_sha256":s.token_program_code_sha256,"tick_array_starts":starts,"max_steps":s.max_steps}},
        "metadata":{"synthetic_proof_identity":true,"deployed_bytecode_claim":false,"bank_commitment":"finalized","bank_slot":s.bank_slot,"bank_blockhash":key("rayclmm synthetic finalized blockhash").to_string(),"program_id":PROGRAM,"programdata":s.programdata,"config":s.amm_config,"config_index":0,"config_bump":cb,"pool_bump":s.pool_bump,"pool_len":PoolState::LEN,"array_len":TickArrayState::LEN,"tick_len":TickState::LEN,"config_len":AmmConfig::LEN,"observation_len":ObservationState::LEN,"pool_discriminator":PoolState::DISCRIMINATOR,"array_discriminator":TickArrayState::DISCRIMINATOR,"observation_discriminator":ObservationState::DISCRIMINATOR,"config_discriminator":AmmConfig::DISCRIMINATOR,"clock":CLOCK,"clock_owner":"Sysvar1111111111111111111111111111111111111","token_program":TOKEN,"immutable_loader":IMMUTABLE,"upgradeable_loader":LOADER,"program_code_utf8":std::str::from_utf8(PROGRAM_CODE).unwrap(),"token_program_code_utf8":std::str::from_utf8(TOKEN_CODE).unwrap(),"program_code_sha256":s.program_code_sha256,"token_program_code_sha256":s.token_program_code_sha256,"pool_mint_keys_sorted_by_bytes":true,"tick_array_starts":starts,"pda_derivation":"Original PoolState::key, TickArrayState::key; Anchor Pubkey::find_program_address with original constants; legacy seed_index=[0,0]","account_bytes":"Original zero-copy packed PoolState/TickArrayState/ObservationState and Anchor Borsh AmmConfig, classic SPL 82/165, loader Program/ProgramData, bincode Clock","scope":"Synthetic finalized RPC fixture, no deployed code or SBF transaction execution"},
        "rpc_accounts":rpc,"rpc_request_accounts":request,"rpc_response":{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":s.bank_slot},"value":values}}
    })
}
fn add_case(cases: &mut Vec<Value>, name: &str, p: Pool, input: u64, d: bool) {
    let e = expected(&p, input, d);
    cases.push(
        json!({"name":name,"pool":p,"input":input.to_string(),"zero_for_one":d,"expected":e}),
    );
}
fn set_fee(p: &mut Pool, fee: u32, protocol: u32, fund: u32) {
    p.fee_numerator = fee as u64;
    p.raydium_clmm.trade_fee_rate = fee;
    p.raydium_clmm.protocol_fee_rate = protocol;
    p.raydium_clmm.fund_fee_rate = fund;
}
fn empty_array(p: &mut Pool, index: usize) {
    let s = &mut p.raydium_clmm;
    let a = &mut s.tick_arrays[index];
    a.ticks.clear();
    let bit = (a.start_tick_index / (60 * s.tick_spacing as i32) + 512) as usize;
    s.bitmap[bit / 64] &= !(1u64 << (bit % 64));
}
fn reset_price(p: &mut Pool, tick: i32) {
    p.raydium_clmm.tick_current = tick;
    p.raydium_clmm.sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick).unwrap().to_string();
}
fn relocate(p: &mut Pool, spacing: u32, start_array: i32, current: i32) {
    let s = &mut p.raydium_clmm;
    s.tick_spacing = spacing;
    s.bitmap.fill(0);
    let span = 60 * spacing as i32;
    for (i, a) in s.tick_arrays.iter_mut().enumerate() {
        let start = (start_array + i as i32) * span;
        a.start_tick_index = start;
        a.address = pda(
            &[
                TICK_ARRAY_SEED.as_bytes(),
                pk(&p.pool_id).as_ref(),
                &start.to_be_bytes(),
            ],
            &pk(PROGRAM),
        )
        .0;
        for (j, t) in a.ticks.iter_mut().enumerate() {
            t.tick = start + (j as i32 * 20) * spacing as i32;
        }
        let bit = (start / span + 512) as usize;
        a.ticks
            .retain(|t| (tick_math::MIN_TICK..=tick_math::MAX_TICK).contains(&t.tick));
        if !a.ticks.is_empty() {
            s.bitmap[bit / 64] |= 1u64 << (bit % 64);
        }
    }
    reset_price(p, current);
}
pub fn generate() -> Value {
    let base = base_pool();
    let mut cases = vec![];
    for (label, fee, protocol, fund) in [
        ("zero-fee", 0, 0, 0),
        ("static-fee", 2500, 120000, 40000),
        ("high-fee", 999999, 250000, 250000),
        ("protocol-all", 3000, 1000000, 0),
        ("fund-all", 3000, 0, 1000000),
        ("lp-only", 10000, 0, 0),
    ] {
        for d in [true, false] {
            for (i, amount) in [
                0,
                1,
                2,
                3,
                10,
                99,
                1000,
                9999,
                100_000,
                1_000_000,
                10_000_000,
                100_000_000,
                499_000_000,
                500_000_000,
                750_000_000,
                1_000_000_000,
                2_000_000_000,
                3_000_000_000,
                4_000_000_000,
                6_000_000_000,
                8_000_000_000,
                10_000_000_000,
                11_000_000_000,
                20_000_000_000,
                100_000_000_000,
                1_000_000_000_000,
                u64::MAX / 2,
                u64::MAX - 1,
                u64::MAX,
            ]
            .into_iter()
            .enumerate()
            {
                let mut p = base.clone();
                set_fee(&mut p, fee, protocol, fund);
                add_case(&mut cases, &format!("{label}-{d}-{i:02}"), p, amount, d);
            }
        }
    }
    for d in [true, false] {
        for (i, amount) in [
            1_000_000,
            1_000_000_000,
            3_000_000_000,
            6_000_000_000,
            10_000_000_000,
            100_000_000_000,
        ]
        .into_iter()
        .enumerate()
        {
            let mut p = base.clone();
            empty_array(&mut p, 3);
            empty_array(&mut p, if d { 2 } else { 4 });
            add_case(&mut cases, &format!("empty-bitmap-{d}-{i}"), p, amount, d);
            let mut p = base.clone();
            p.raydium_clmm.liquidity = "0".into();
            let a = if d { 2 } else { 4 };
            p.raydium_clmm.tick_arrays[a].ticks[0].liquidity_net =
                if d { "-1000000000000" } else { "1000000000000" }.into();
            empty_array(&mut p, 3);
            add_case(
                &mut cases,
                &format!("zero-liquidity-skip-{d}-{i}"),
                p,
                amount,
                d,
            );
            let mut p = base.clone();
            p.raydium_clmm.fee_growth_global0 = (u128::MAX - 10).to_string();
            p.raydium_clmm.fee_growth_global1 = (u128::MAX - 20).to_string();
            for a in &mut p.raydium_clmm.tick_arrays {
                for t in &mut a.ticks {
                    t.fee_growth_outside0 = (u128::MAX - 100).to_string();
                    t.fee_growth_outside1 = (u128::MAX - 200).to_string();
                }
            }
            add_case(&mut cases, &format!("growth-wrap-{d}-{i}"), p, amount, d);
        }
        for (kind, liquidity) in [
            ("u128-max", u128::MAX),
            ("u128-above-u64", u64::MAX as u128 + 1),
            ("u64-max", u64::MAX as u128),
            ("tiny-liquidity", 1),
        ] {
            let mut p = base.clone();
            p.raydium_clmm.liquidity = liquidity.to_string();
            add_case(&mut cases, &format!("{kind}-{d}"), p, 1_000_000_000, d);
        }
        for kind in [
            "protocol-counter-overflow",
            "fund-counter-overflow",
            "raw-input-overflow",
            "raw-output-underflow",
            "observation-wrap",
            "observation-too-soon",
            "observation-time-backwards",
            "future-open-time",
            "clock-u32-max",
            "clock-out-of-scope",
            "max-steps-zero",
            "partial-window-exhaustion",
        ] {
            let mut p = base.clone();
            let s = &mut p.raydium_clmm;
            match kind {
                "protocol-counter-overflow" => {
                    if d {
                        s.protocol_fees0 = u64::MAX
                    } else {
                        s.protocol_fees1 = u64::MAX
                    }
                }
                "fund-counter-overflow" => {
                    if d {
                        s.fund_fees0 = u64::MAX
                    } else {
                        s.fund_fees1 = u64::MAX
                    }
                }
                "raw-input-overflow" => {
                    if d {
                        s.raw_vault0 = u64::MAX
                    } else {
                        s.raw_vault1 = u64::MAX
                    }
                }
                "raw-output-underflow" => {
                    if d {
                        s.raw_vault1 = 1
                    } else {
                        s.raw_vault0 = 1
                    }
                }
                "observation-wrap" => {
                    s.observation_initialized = true;
                    s.observation_index = 99;
                    s.observations[99] = ObservationJSON {
                        block_timestamp: s.bank_unix_timestamp as u32 - 30,
                        tick_cumulative: i64::MAX - 5,
                    };
                }
                "observation-too-soon" => {
                    s.observation_initialized = true;
                    s.observations[0] = ObservationJSON {
                        block_timestamp: s.bank_unix_timestamp as u32 - 14,
                        tick_cumulative: 9,
                    };
                }
                "observation-time-backwards" => {
                    s.observation_initialized = true;
                    s.observations[0] = ObservationJSON {
                        block_timestamp: s.bank_unix_timestamp as u32 + 1,
                        tick_cumulative: 9,
                    };
                }
                "future-open-time" => s.open_time = s.bank_unix_timestamp,
                "clock-u32-max" => s.bank_unix_timestamp = u32::MAX as u64,
                "clock-out-of-scope" => s.bank_unix_timestamp = u32::MAX as u64 + 1,
                "max-steps-zero" => s.max_steps = 0,
                _ => {}
            }
            add_case(
                &mut cases,
                &format!("{kind}-{d}"),
                p,
                if kind == "partial-window-exhaustion" {
                    u64::MAX
                } else {
                    1_000_000_000
                },
                d,
            );
        }
        for tick in [-120, -60, -1, 0, 1, 20, 59, 60, 120] {
            let mut p = base.clone();
            reset_price(&mut p, tick);
            add_case(
                &mut cases,
                &format!("tick-boundary-{tick}-{d}"),
                p,
                1_000_000_000,
                d,
            );
        }
        for (kind, spacing, start, tick) in [
            ("bitmap-low-edge", 1, -512, -30550),
            ("bitmap-high-edge", 1, 505, 30590),
            ("min-tick", 15, -493, -443630),
            ("max-tick", 15, 486, 443630),
            ("wide-spacing", 10, -3, 10),
        ] {
            let mut p = base.clone();
            relocate(&mut p, spacing, start, tick);
            add_case(&mut cases, &format!("{kind}-{d}"), p, 1_000_000_000, d);
        }
        let mut p = base.clone();
        for a in &mut p.raydium_clmm.tick_arrays {
            for t in &mut a.ticks {
                t.liquidity_net = if t.tick < 0 {
                    "-1000000000"
                } else {
                    "1000000000"
                }
                .into();
            }
        }
        add_case(
            &mut cases,
            &format!("cross-liquidity-delta-{d}"),
            p,
            5_000_000_000,
            d,
        );
    }
    // Successful full-u64 input/output probes retain JSON integer fidelity.
    for d in [true, false] {
        for (i, amount) in [
            (1u64 << 53) - 1,
            1u64 << 53,
            (1u64 << 53) + 1,
            (1u64 << 63) - 1,
            1u64 << 63,
            u64::MAX - 2,
            u64::MAX - 1,
            u64::MAX,
        ]
        .into_iter()
        .enumerate()
        {
            for fee in [0, 2500] {
                let mut p = base.clone();
                set_fee(&mut p, fee, 120000, 40000);
                let s = &mut p.raydium_clmm;
                s.liquidity = "1000000000000000000000000".into();
                if d {
                    s.raw_vault0 = 0;
                    s.raw_vault1 = u64::MAX;
                    s.protocol_fees0 = 0;
                    s.fund_fees0 = 0;
                } else {
                    s.raw_vault1 = 0;
                    s.raw_vault0 = u64::MAX;
                    s.protocol_fees1 = 0;
                    s.fund_fees1 = 0;
                }
                add_case(
                    &mut cases,
                    &format!("u64-full-width-{d}-{fee}-{i}"),
                    p,
                    amount,
                    d,
                );
            }
        }
    }
    for (label, start, current, d) in [
        ("min-price-success", -493, -443630, false),
        ("max-price-success", 486, 443630, true),
    ] {
        let mut p = base.clone();
        relocate(&mut p, 15, start, current);
        set_fee(&mut p, 0, 0, 0);
        if d {
            p.raydium_clmm.raw_vault0 = 0;
            p.raydium_clmm.raw_vault1 = u64::MAX;
        } else {
            p.raydium_clmm.raw_vault1 = 0;
            p.raydium_clmm.raw_vault0 = u64::MAX;
        }
        add_case(&mut cases, label, p, 1, d);
    }
    let mut sequences = vec![];
    for i in 0..12 {
        let mut p = base.clone();
        if i % 3 == 0 {
            set_fee(&mut p, 0, 0, 0);
        }
        if i % 3 == 2 {
            p.raydium_clmm.observation_initialized = true;
            p.raydium_clmm.observations[0] = ObservationJSON {
                block_timestamp: p.raydium_clmm.bank_unix_timestamp as u32 - 30,
                tick_cumulative: i64::MAX - 20,
            };
        }
        let initial = p.clone();
        let mut steps = vec![];
        for j in 0..8 {
            let d = if i < 4 {
                true
            } else if i < 8 {
                false
            } else {
                j % 2 == 0
            };
            let input = if j % 2 == 0 {
                100_000_000u64
            } else {
                200_000_000u64
            };
            let q = quote(&p, &input.to_string(), d).expect("sequence quote");
            steps.push(json!({"input":input.to_string(),"zero_for_one":d,"expected":{"amount_out":q.amount_out.to_string(),"next_pool":q.pool}}));
            p = q.pool;
        }
        sequences.push(json!({"name":format!("{}-{i}",if i<8{"same-direction"}else{"mixed-direction"}),"pool":initial,"steps":steps,"expected_final_pool":p}));
    }
    json!({"reference":{"repository":"raydium-io/raydium-clmm","revision":REVISION,"program_id":PROGRAM,"execution":"unchanged upstream no-entrypoint swap_internal + original state helpers, RefMut accounts, deterministic native Solana Clock SyscallStubs","business_math_imported":false,"synthetic_proof_identity":true,"sbf_executed":false,"scope":"fee_on=0; DynamicFeeInfo default; legacy seed; no rewards/orders; classic SPL; base bitmap; u32 bank time; full exact input","partial_behavior":"no-limit entrypoint rejects partial fill; proof-window exhaustion and missing extension revert"},"base_pool":base,"cases":cases,"sequences":sequences})
}
/// Dedicated token-fee corpus. Only scenario inputs are inherited from the static
/// corpus; every expected result is executed again through the original program.
pub fn generate_token_fee() -> Value {
    let scenarios = generate();
    let mut cases = vec![];
    let mut sequences = vec![];
    let mut independent_vectors = vec![];
    let token_pool = |mut pool: Pool, mode: u32| {
        pool.protocol = TOKEN_FEE_PROFILE.into();
        pool.adapter_version = TOKEN_FEE_ADAPTER.into();
        pool.raydium_clmm.collect_fee_on = mode;
        pool
    };
    for mode in [1, 2] {
        for scenario in scenarios["cases"].as_array().unwrap() {
            let mut pool = token_pool(
                serde_json::from_value(scenario["pool"].clone()).unwrap(),
                mode,
            );
            let name = scenario["name"].as_str().unwrap();
            // The old overflow probes target the input token. Point these probes
            // at the selected fee token, including when that token is the output.
            let s = &mut pool.raydium_clmm;
            if name.starts_with("protocol-counter-overflow-") {
                if mode == 1 {
                    s.protocol_fees0 = u64::MAX;
                } else {
                    s.protocol_fees1 = u64::MAX;
                }
            }
            if name.starts_with("fund-counter-overflow-") {
                if mode == 1 {
                    s.fund_fees0 = u64::MAX;
                } else {
                    s.fund_fees1 = u64::MAX;
                }
            }
            add_case(
                &mut cases,
                &format!("mode{mode}-{name}"),
                pool,
                scenario["input"].as_str().unwrap().parse().unwrap(),
                scenario["zero_for_one"].as_bool().unwrap(),
            );
        }
        for direction in [true, false] {
            for input in [1_000_000, 2_000_000_000] {
                let name = format!("mode{mode}-independent-{input}-{direction}");
                add_case(
                    &mut cases,
                    &name,
                    token_pool(base_pool(), mode),
                    input,
                    direction,
                );
                independent_vectors.push(name);
            }
        }
        for (index, scenario) in scenarios["sequences"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            // Cover both alternating orders while retaining the inherited amounts.
            let reverse_mixed = scenario["name"]
                .as_str()
                .unwrap()
                .starts_with("mixed-direction-")
                && index % 2 == 1;
            let mut pool = token_pool(
                serde_json::from_value(scenario["pool"].clone()).unwrap(),
                mode,
            );
            let initial = pool.clone();
            let mut steps = vec![];
            for step in scenario["steps"].as_array().unwrap() {
                let input = step["input"].as_str().unwrap();
                let direction = step["zero_for_one"].as_bool().unwrap() ^ reverse_mixed;
                let q = quote(&pool, input, direction).expect("token-fee sequence quote");
                steps.push(json!({"input":input,"zero_for_one":direction,"expected":{"amount_out":q.amount_out.to_string(),"next_pool":q.pool}}));
                pool = q.pool;
            }
            sequences.push(json!({"name":format!("mode{mode}-{}",scenario["name"].as_str().unwrap()),"pool":initial,"steps":steps,"expected_final_pool":pool}));
        }
    }
    json!({
        "reference": {
            "repository":"raydium-io/raydium-clmm",
            "revision":REVISION,
            "program_id":PROGRAM,
            "profile":TOKEN_FEE_PROFILE,
            "adapter_version":TOKEN_FEE_ADAPTER,
            "collect_fee_on":[1,2],
            "execution":"unchanged upstream no-entrypoint swap_internal + original state helpers, PoolState::set_fee_on, RefMut accounts, deterministic native Solana Clock SyscallStubs",
            "business_math_imported":false,
            "synthetic_proof_identity":true,
            "sbf_executed":false,
            "token_program_executed":false,
            "scope":"fee_on=1 (Token0Only) or 2 (Token1Only); DynamicFeeInfo default; legacy seed; no rewards/orders; classic SPL; base bitmap; u32 bank time; full exact input",
            "partial_behavior":"no-limit entrypoint rejects partial fill; proof-window exhaustion and missing extension revert",
            "execution_limits": {
                "max_steps":"1..=8192; conservative preflight counts all initialized ticks plus one final partial step; stricter admission than native execution; not a runtime instruction budget meter",
                "bank_time":"open_time < bank_unix_timestamp <= u32::MAX",
                "accounts":"3..=7 contiguous base-bitmap arrays; no bitmap extension; missing proof arrays revert",
                "execution":"native core and Clock syscall only; Anchor account validation, SBF, CPI, SPL transfers, signatures, owner/executable checks, user balances and WSOL backing are not executed",
                "vaults":"classic SPL transfer deltas modeled with checked u64 full-input addition and net-output subtraction",
                "boundary_states":"some counter and width probes intentionally use states not reachable from the base fixture",
                "identity":"synthetic proof code is not deployed bytecode; pinned source does not establish deployed program identity"
            }
        },
        "base_pool":token_pool(base_pool(),1),
        "base_pool_mode2":token_pool(base_pool(),2),
        "independent_vectors":independent_vectors,
        "cases":cases,
        "sequences":sequences
    })
}

pub fn tick_vectors() -> Value {
    let mut ticks = std::collections::BTreeSet::new();
    for i in 0..=2048 {
        ticks.insert(
            tick_math::MIN_TICK
                + (i as i64 * (tick_math::MAX_TICK - tick_math::MIN_TICK) as i64 / 2048) as i32,
        );
    }
    ticks.extend([-443636, -443635, -1, 0, 1, 443635, 443636]);
    let vectors:Vec<_>=ticks.into_iter().map(|tick|{let price=tick_math::get_sqrt_price_at_tick(tick).unwrap();let inverse=|p|tick_math::get_tick_at_sqrt_price(p).ok();json!({"tick":tick,"sqrt_price_x64":price.to_string(),"tick_from_price":inverse(price),"tick_below":inverse(price.saturating_sub(1)),"tick_above":inverse(price+1)})}).collect();
    let mut digest = Sha256::new();
    for tick in tick_math::MIN_TICK..=tick_math::MAX_TICK {
        digest.update(tick.to_le_bytes());
        digest.update(
            tick_math::get_sqrt_price_at_tick(tick)
                .unwrap()
                .to_le_bytes(),
        );
    }
    json!({"revision":REVISION,"source_function":"libraries::tick_math::{get_sqrt_price_at_tick,get_tick_at_sqrt_price}","vectors":vectors,"exhaustive":{"min_tick":tick_math::MIN_TICK,"max_tick":tick_math::MAX_TICK,"count":tick_math::MAX_TICK-tick_math::MIN_TICK+1,"encoding":"ascending tick i32 little-endian followed by source sqrt_price u128 little-endian","sha256":format!("{:x}",digest.finalize())}})
}

#[cfg(test)]
mod original_behavior_tests {
    use super::*;

    #[test]
    fn original_core_can_partially_fill_explicit_limit_but_profile_requires_full_input() {
        let p = base_pool();
        let _lock = NATIVE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _stubs = install(&p.raydium_clmm);
        let (config, state, arrays, obs) = original_states(&p);
        let state = RefCell::new(state);
        let obs = RefCell::new(obs);
        let arrays: Vec<_> = arrays.into_iter().map(RefCell::new).collect();
        let mut queue: VecDeque<_> = arrays.iter().rev().map(RefCell::borrow_mut).collect();
        let specified = 1_000_000_000_000;
        let result = swap_internal(
            &config,
            &mut state.borrow_mut(),
            &mut queue,
            &mut obs.borrow_mut(),
            None,
            specified,
            tick_math::get_sqrt_price_at_tick(0).unwrap(),
            true,
            true,
            p.raydium_clmm.bank_unix_timestamp as u32,
        )
        .unwrap();
        assert!(result.amount_0 > 0 && result.amount_0 < specified);
        assert!(result.amount_1 > 0);
        assert_eq!(
            { state.borrow().sqrt_price_x64 },
            tick_math::get_sqrt_price_at_tick(0).unwrap()
        );
        drop(queue);
        drop(_stubs);
        drop(_lock);
        assert!(quote(&p, &specified.to_string(), true).is_err());
    }

    #[test]
    fn original_requires_extension_when_base_bitmap_has_no_next_liquidity() {
        let mut p = base_pool();
        for i in 0..p.raydium_clmm.tick_arrays.len() {
            empty_array(&mut p, i);
        }
        let error = quote(&p, "1000000000", true).err().unwrap();
        assert!(
            error.contains("MissingTickArrayBitmapExtensionAccount"),
            "{error}"
        );
    }
}

pub fn reward_update(pool: &Pool, outside: &[u128;3]) -> Value {
    let _lock=NATIVE_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let _stubs=install(&pool.raydium_clmm);
    let (_,mut state,_,_)=original_states(pool);
    let rewards=state.update_reward_infos(pool.raydium_clmm.bank_unix_timestamp).unwrap();
    let mut tick=TickState{reward_growths_outside_x64:*outside,..TickState::default()};
    tick.cross(0,0,&rewards);
    let crossed=tick.reward_growths_outside_x64;
    json!({"reward_infos":rewards.into_iter().map(Reward::from_original).collect::<Vec<_>>(),
      "outside":crossed.iter().map(|v|v.to_string()).collect::<Vec<_>>()})
}
