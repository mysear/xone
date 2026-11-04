//! Independent native oracle: all AMM fee/curve/ledger operations call unchanged upstream.
use anchor_lang::prelude::{Clock, Pubkey};
use raydium_cp_swap::{
    curve::{CurveCalculator, TradeDirection},
    states::{AmmConfig, PoolState},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    str::FromStr,
};

pub const REVISION: &str = "f4b9b26342afe9122d5ff67cd7a4f19ded4ac0a5";
pub const PROGRAM: &str = "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C";
pub const PROFILE: &str = "raydium-cpmm-creator";
pub const ADAPTER: &str = "raydium-cpmm/f4b9b263-spl-1";
pub const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const SYNTHETIC_PROGRAM_CODE: &[u8] =
    b"XONE synthetic Raydium proof fixture; NOT deployed program bytecode; f4b9b263";
pub const SYNTHETIC_TOKEN_CODE: &[u8] =
    b"XONE synthetic classic SPL Token proof fixture; NOT deployed token bytecode";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct State {
    pub program_id: String,
    pub programdata: String,
    pub program_code_sha256: String,
    pub deployed_slot: u64,
    pub token_program_code_sha256: String,
    pub authority: String,
    pub authority_bump: u32,
    pub amm_config: String,
    pub vault0: String,
    pub vault1: String,
    pub raw_vault0: u64,
    pub raw_vault1: u64,
    pub protocol_fee_rate: u64,
    pub fund_fee_rate: u64,
    pub creator_fee_rate: u64,
    pub creator_fee_share_rate: u64,
    pub creator_fee_on: u32,
    pub creator_fee_enabled: bool,
    pub protocol_fees0: u64,
    pub protocol_fees1: u64,
    pub fund_fees0: u64,
    pub fund_fees1: u64,
    pub creator_fees0: u64,
    pub creator_fees1: u64,
    pub bank_slot: u64,
    pub bank_unix_timestamp: u64,
    pub bank_epoch: u64,
    pub open_time: u64,
    pub recent_epoch: u64,
    pub mint0_decimals: u32,
    pub mint1_decimals: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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
    pub raydium_cpmm: State,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Fees {
    pub input_amount: String,
    pub trade_fee: u64,
    pub protocol_fee: u64,
    pub fund_fee: u64,
    pub creator_fee: u64,
    pub creator_fee_on_input: bool,
    pub curve_new_input: String,
    pub curve_new_output: String,
    pub constant_before: String,
    pub curve_constant_after: String,
    pub actual_net_constant_after: String,
}

#[derive(Clone, Debug)]
pub struct Quote {
    pub amount_out: u64,
    pub pool: Pool,
    pub fees: Fees,
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn key(label: &str) -> Pubkey {
    Pubkey::new_from_array(Sha256::digest(label.as_bytes()).into())
}
fn pk(s: &str) -> Result<Pubkey, String> {
    let p = Pubkey::from_str(s).map_err(|e| e.to_string())?;
    if p.to_string() != s || p == Pubkey::default() {
        return Err("noncanonical public key".into());
    }
    Ok(p)
}
fn pda(seeds: &[&[u8]], program: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(seeds, program)
}

pub fn base_pool() -> Pool {
    let program = pk(PROGRAM).unwrap();
    let (authority, bump) = pda(&[raydium_cp_swap::AUTH_SEED.as_bytes()], &program);
    let (config, _) = pda(&[b"amm_config", &0u16.to_be_bytes()], &program);
    let mut mints = [key("xone:raydium:mint:A"), key("xone:raydium:mint:B")];
    mints.sort_by_key(|m| m.to_bytes());
    let (pool, _) = pda(
        &[
            b"pool",
            config.as_ref(),
            mints[0].as_ref(),
            mints[1].as_ref(),
        ],
        &program,
    );
    let vault = |mint: &Pubkey| {
        pda(&[b"pool_vault", pool.as_ref(), mint.as_ref()], &program)
            .0
            .to_string()
    };
    let loader = pk("BPFLoaderUpgradeab1e11111111111111111111111").unwrap();
    let programdata = pda(&[program.as_ref()], &loader).0;
    Pool {
        chain_key: "solana:localnet".into(),
        pool_id: pool.to_string(),
        protocol: PROFILE.into(),
        adapter_version: ADAPTER.into(),
        token0: mints[0].to_string(),
        token1: mints[1].to_string(),
        reserve0: "1000000000000".into(),
        reserve1: "1700000000000".into(),
        fee_numerator: 2500,
        fee_denominator: 1_000_000,
        token_behavior: "standard".into(),
        raydium_cpmm: State {
            program_id: PROGRAM.into(),
            programdata: programdata.to_string(),
            program_code_sha256: sha256(SYNTHETIC_PROGRAM_CODE),
            deployed_slot: 200,
            token_program_code_sha256: sha256(SYNTHETIC_TOKEN_CODE),
            authority: authority.to_string(),
            authority_bump: bump as u32,
            amm_config: config.to_string(),
            vault0: vault(&mints[0]),
            vault1: vault(&mints[1]),
            raw_vault0: 1_000_000_000_000 + 101 + 211 + 307,
            raw_vault1: 1_700_000_000_000 + 103 + 223 + 311,
            protocol_fee_rate: 120_000,
            fund_fee_rate: 40_000,
            creator_fee_rate: 500,
            creator_fee_share_rate: 250_000,
            creator_fee_on: 0,
            creator_fee_enabled: true,
            protocol_fees0: 101,
            protocol_fees1: 103,
            fund_fees0: 211,
            fund_fees1: 223,
            creator_fees0: 307,
            creator_fees1: 311,
            bank_slot: 123456,
            bank_unix_timestamp: 1792972800,
            bank_epoch: 987,
            open_time: 1,
            recent_epoch: 986,
            mint0_decimals: 9,
            mint1_decimals: 6,
        },
    }
}

fn native(s: &State) -> Result<PoolState, String> {
    let mut n = PoolState::default();
    n.amm_config = pk(&s.amm_config)?;
    n.token_0_vault = pk(&s.vault0)?;
    n.token_1_vault = pk(&s.vault1)?;
    n.auth_bump = u8::try_from(s.authority_bump).map_err(|e| e.to_string())?;
    n.protocol_fees_token_0 = s.protocol_fees0;
    n.protocol_fees_token_1 = s.protocol_fees1;
    n.fund_fees_token_0 = s.fund_fees0;
    n.fund_fees_token_1 = s.fund_fees1;
    n.creator_fees_token_0 = s.creator_fees0;
    n.creator_fees_token_1 = s.creator_fees1;
    n.creator_fee_on = u8::try_from(s.creator_fee_on).map_err(|e| e.to_string())?;
    n.enable_creator_fee = s.creator_fee_enabled;
    n.open_time = s.open_time;
    n.recent_epoch = s.recent_epoch;
    Ok(n)
}

fn validate(p: &Pool) -> Result<(), String> {
    let s = &p.raydium_cpmm;
    if p.protocol != PROFILE
        || p.adapter_version != ADAPTER
        || p.chain_key != "solana:localnet"
        || p.token_behavior != "standard"
        || p.fee_denominator != 1_000_000
        || s.program_id != PROGRAM
    {
        return Err("unsupported profile, identity, chain, token behavior or denominator".into());
    }
    // Broader metadata bounds permit arithmetic probes outside public configuration admission.
    // At pinned f4b9b263, upstream src/lib.rs:106 checks trade + creator < 1_000_000
    // before calling the internal initializer; update_config.rs:49/:60 preserves it.
    // Accepting a broader configured sum here tests helper branch arithmetic only;
    // it does not establish a reachable configuration or deployed admitted state.
    if p.fee_numerator >= 1_000_000
        || s.creator_fee_rate > 1_000_000
        || s.protocol_fee_rate
            .checked_add(s.fund_fee_rate)
            .filter(|v| *v <= 1_000_000)
            .is_none()
        || s.creator_fee_share_rate > 1_000_000
        || s.creator_fee_on > 2
        || s.mint0_decimals > 255
        || s.mint1_decimals > 255
    {
        return Err("invalid creator mode or config fee bound".into());
    }
    let program = pk(&s.program_id)?;
    let (a, b) = pda(&[raydium_cp_swap::AUTH_SEED.as_bytes()], &program);
    if s.authority != a.to_string() || s.authority_bump != u32::from(b) {
        return Err("invalid global authority PDA".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for x in [
        p.pool_id.as_str(),
        &p.token0,
        &p.token1,
        &s.programdata,
        &s.amm_config,
        &s.vault0,
        &s.vault1,
        &s.authority,
        PROGRAM,
        TOKEN_PROGRAM,
        "SysvarC1ock11111111111111111111111111111111",
    ] {
        pk(x)?;
        if !seen.insert(x) {
            return Err("aliased account role".into());
        }
    }
    if pk(&p.token0)?.to_bytes() >= pk(&p.token1)?.to_bytes() {
        return Err("mint byte order".into());
    }
    for hash in [&s.program_code_sha256, &s.token_program_code_sha256] {
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err("code SHA256 format".into());
        }
    }
    if s.deployed_slot > s.bank_slot
        || s.bank_unix_timestamp < s.open_time
        || s.bank_unix_timestamp > i64::MAX as u64
    {
        return Err("invalid bank/open/deployed time".into());
    }
    Ok(())
}

pub fn quote(p: &Pool, input: &str, zero_for_one: bool) -> Result<Quote, String> {
    // Upstream checked arithmetic occasionally unwraps. The native oracle treats those panics
    // as atomic transaction rejection, without publishing any partially mutated state.
    catch_unwind(AssertUnwindSafe(|| quote_inner(p, input, zero_for_one)))
        .map_err(|_| "original program arithmetic panic (transaction rollback)".to_string())?
}

fn quote_inner(p: &Pool, input: &str, zero_for_one: bool) -> Result<Quote, String> {
    validate(p)?;
    let amount: u64 = input
        .parse::<u64>()
        .map_err(|_| "input outside u64".to_string())?;
    if amount == 0 {
        return Err("original swap requires positive actual input".into());
    }
    let mut n = native(&p.raydium_cpmm)?;
    let s = &p.raydium_cpmm;
    let (r0, r1) = n
        .vault_amount_without_fee(s.raw_vault0, s.raw_vault1)
        .map_err(|e| e.to_string())?;
    if p.reserve0 != r0.to_string() || p.reserve1 != r1.to_string() {
        return Err("reserve/ledger mismatch".into());
    }
    // get_swap_params also computes the original prices, including rejecting zero reserve division.
    let (iv, ov, iraw, oraw) = if zero_for_one {
        (n.token_0_vault, n.token_1_vault, s.raw_vault0, s.raw_vault1)
    } else {
        (n.token_1_vault, n.token_0_vault, s.raw_vault1, s.raw_vault0)
    };
    let params = n
        .get_swap_params(iv, ov, iraw, oraw)
        .map_err(|e| e.to_string())?;
    let dir = if zero_for_one {
        TradeDirection::ZeroForOne
    } else {
        TradeDirection::OneForZero
    };
    let on_input = n.is_creator_fee_on_input(dir).map_err(|e| e.to_string())?;
    let creator_rate = n.adjust_creator_fee_rate(s.creator_fee_rate);
    let r = CurveCalculator::swap_base_input(
        amount.into(),
        params.total_input_token_amount.into(),
        params.total_output_token_amount.into(),
        p.fee_numerator,
        creator_rate,
        s.protocol_fee_rate,
        s.fund_fee_rate,
        on_input,
    )
    .ok_or("original CurveCalculator returned None")?;
    if r.input_amount != u128::from(amount) {
        return Err("input not fully consumed".into());
    }
    let output = u64::try_from(r.output_amount).map_err(|_| "output outside u64")?;
    if output == 0 {
        return Err("original swap requires positive amount received".into());
    }
    let before = u128::from(r0)
        .checked_mul(u128::from(r1))
        .ok_or("pre invariant overflow")?;
    let curve_after = r
        .new_input_vault_amount
        .checked_mul(r.new_output_vault_amount)
        .ok_or("curve invariant overflow")?;
    if curve_after < before {
        return Err("original invariant check failed".into());
    }
    let protocol = u64::try_from(r.protocol_fee).map_err(|_| "protocol fee outside u64")?;
    let fund = u64::try_from(r.fund_fee).map_err(|_| "fund fee outside u64")?;
    let creator = u64::try_from(r.creator_fee).map_err(|_| "creator fee outside u64")?;
    n.update_fees(protocol, fund, creator, dir)
        .map_err(|e| e.to_string())?;
    let mut next = p.clone();
    let ns = &mut next.raydium_cpmm;
    if zero_for_one {
        ns.raw_vault0 = s
            .raw_vault0
            .checked_add(amount)
            .ok_or("SPL input vault overflow")?;
        ns.raw_vault1 = s
            .raw_vault1
            .checked_sub(output)
            .ok_or("SPL output vault underflow")?;
    } else {
        ns.raw_vault1 = s
            .raw_vault1
            .checked_add(amount)
            .ok_or("SPL input vault overflow")?;
        ns.raw_vault0 = s
            .raw_vault0
            .checked_sub(output)
            .ok_or("SPL output vault underflow")?;
    }
    ns.protocol_fees0 = n.protocol_fees_token_0;
    ns.protocol_fees1 = n.protocol_fees_token_1;
    ns.fund_fees0 = n.fund_fees_token_0;
    ns.fund_fees1 = n.fund_fees_token_1;
    ns.creator_fees0 = n.creator_fees_token_0;
    ns.creator_fees1 = n.creator_fees_token_1;
    let clock = Clock {
        slot: s.bank_slot,
        epoch_start_timestamp: 0,
        epoch: s.bank_epoch,
        leader_schedule_epoch: s.bank_epoch,
        unix_timestamp: s.bank_unix_timestamp as i64,
    };
    n.recent_epoch = clock.epoch; // exact assignment in upstream swap_base_input, no syscall or SBF transaction.
    ns.recent_epoch = n.recent_epoch;
    let (nr0, nr1) = n
        .vault_amount_without_fee(ns.raw_vault0, ns.raw_vault1)
        .map_err(|e| e.to_string())?;
    next.reserve0 = nr0.to_string();
    next.reserve1 = nr1.to_string();
    let actual_after = u128::from(nr0) * u128::from(nr1);
    if actual_after < before {
        return Err("actual net invariant decreased".into());
    }
    Ok(Quote {
        amount_out: output,
        pool: next,
        fees: Fees {
            input_amount: r.input_amount.to_string(),
            trade_fee: u64::try_from(r.trade_fee).map_err(|_| "trade fee outside u64")?,
            protocol_fee: protocol,
            fund_fee: fund,
            creator_fee: creator,
            creator_fee_on_input: on_input,
            curve_new_input: r.new_input_vault_amount.to_string(),
            curve_new_output: r.new_output_vault_amount.to_string(),
            constant_before: before.to_string(),
            curve_constant_after: curve_after.to_string(),
            actual_net_constant_after: actual_after.to_string(),
        },
    })
}

fn refresh(p: &mut Pool) {
    let s = &p.raydium_cpmm;
    let n = native(s).unwrap();
    if let Ok((a, b)) = n.vault_amount_without_fee(s.raw_vault0, s.raw_vault1) {
        p.reserve0 = a.to_string();
        p.reserve1 = b.to_string();
    }
}
fn reserves(p: &mut Pool, r0: u64, r1: u64) {
    let s = &mut p.raydium_cpmm;
    s.raw_vault0 = r0
        .checked_add(s.protocol_fees0)
        .and_then(|v| v.checked_add(s.fund_fees0))
        .and_then(|v| v.checked_add(s.creator_fees0))
        .unwrap();
    s.raw_vault1 = r1
        .checked_add(s.protocol_fees1)
        .and_then(|v| v.checked_add(s.fund_fees1))
        .and_then(|v| v.checked_add(s.creator_fees1))
        .unwrap();
    refresh(p);
}
fn case(name: String, p: &Pool, input: &str, dir: bool) -> Value {
    match quote(p, input, dir) {
        Ok(q) => {
            json!({"name":name,"pool":p,"input":input,"zero_for_one":dir,"expected":{"amount_out":q.amount_out.to_string(),"reserve0":q.pool.reserve0,"reserve1":q.pool.reserve1,"raydium_cpmm":q.pool.raydium_cpmm},"raw_reference":q.fees})
        }
        Err(reason) => {
            json!({"name":name,"pool":p,"input":input,"zero_for_one":dir,"expected":{"reverted":true},"raw_reference":{"revert_reason":reason}})
        }
    }
}

/// These test the actual calculator's u128 checked/unwrap boundaries and include
/// inputs beyond SPL u64 or the public creation/update configuration bounds.
/// They are broader arithmetic probes, never evidence of configuration admission
/// or reachable token transactions, including when the helper returns Some.
pub fn core_boundaries() -> Value {
    let m = u128::MAX;
    let rows: Vec<(&str, u128, u128, u128, u64, u64, bool)> = vec![
        (
            "u64_cube_product_safe",
            u64::MAX.into(),
            u64::MAX.into(),
            u64::MAX.into(),
            2500,
            500,
            true,
        ),
        ("u128_fee_mul_overflow", m, 1, 1, 2500, 500, true),
        ("u128_fee_ceil_add_overflow", m - 1, 1, 1, 1, 0, true),
        (
            "u128_curve_numerator_overflow",
            m / 2 + 1,
            1,
            3,
            0,
            0,
            false,
        ),
        ("u128_curve_denominator_overflow", 10, m - 5, 1, 0, 0, false),
        ("u128_curve_zero_denominator", 0, 0, 1, 0, 0, false),
        ("u128_zero_rate_split", 100, 1000, 1000, 0, 0, true),
        ("u128_zero_rate_output_branch", 100, 1000, 1000, 0, 0, false),
        ("u128_new_input_near_max", 1, m - 1, 1, 0, 0, false),
        (
            "u128_fee_exact_boundary",
            m / 1_000_000 - 1,
            100,
            100,
            1,
            0,
            true,
        ),
        ("fee_exceeds_input", 1, 1000, 1000, 999999, 999999, true),
        (
            "output_creator_exceeds_output",
            10000,
            1000,
            1000,
            0,
            2000000,
            false,
        ),
        (
            "input_rate_u64_add_overflow",
            100,
            1000,
            1000,
            u64::MAX,
            u64::MAX,
            true,
        ),
    ];
    Value::Array(rows.into_iter().map(|(name,i,x,y,t,c,on)| {
        let call = catch_unwind(|| CurveCalculator::swap_base_input(i,x,y,t,c,120000,40000,on));
        let (expected, kind) = match call {
            Ok(Some(r)) => (json!({"input_amount":r.input_amount.to_string(),"output_amount":r.output_amount.to_string(),"new_input_vault_amount":r.new_input_vault_amount.to_string(),"new_output_vault_amount":r.new_output_vault_amount.to_string(),"trade_fee":r.trade_fee.to_string(),"protocol_fee":r.protocol_fee.to_string(),"fund_fee":r.fund_fee.to_string(),"creator_fee":r.creator_fee.to_string()}), "Some"),
            Ok(None) => (json!({"reverted":true}),"None"),
            Err(_) => (json!({"reverted":true}),"panic"),
        };
        json!({"name":name,"input":i.to_string(),"input_vault":x.to_string(),"output_vault":y.to_string(),"trade_fee_rate":t,"creator_fee_rate":c,"protocol_fee_rate":120000,"fund_fee_rate":40000,"creator_fee_on_input":on,"expected":expected,"original_outcome":kind,"domain":"calculator-only u128; may be unreachable SPL state"})
    }).collect())
}

pub fn generate() -> Value {
    let base = base_pool();
    assert_eq!(PoolState::LEN, 637);
    assert_eq!(AmmConfig::LEN, 236);
    assert_eq!(std::mem::size_of::<PoolState>(), 629);
    let mut cases = Vec::new();
    let schedules = [
        (2500, 500, 120000, 40000),
        (0, 0, 0, 0),
        (0, 500, 1000000, 0),
        (2500, 0, 0, 1000000),
        (1, 1, 333333, 666667),
        (499999, 500000, 1, 1),
        (12345, 6789, 0, 0),
        // Schedules 7..=9 are broader arithmetic probes: configured trade + creator
        // meets/exceeds the public creation/update limit, even when creator is disabled.
        // Preserve serialized names and expected helper outcomes; no reachability claim.
        (600000, 600000, 120000, 40000),
        (0, 1000000, 0, 0),
        (999999, 1000000, 0, 0),
    ];
    let sizes = [
        "0",
        "1",
        "2",
        "333",
        "999",
        "1000",
        "1001",
        "100000",
        "1000003",
        "9007199254740993",
        "18446744073709551615",
        "18446744073709551616",
    ];
    let mut bases = Vec::new();
    for mode in 0..3 {
        for enabled in [true, false] {
            for (j, (t, c, p, f)) in schedules.iter().copied().enumerate() {
                let mut pool = base.clone();
                pool.fee_numerator = t;
                let s = &mut pool.raydium_cpmm;
                s.creator_fee_on = mode;
                s.creator_fee_enabled = enabled;
                s.creator_fee_rate = c;
                s.protocol_fee_rate = p;
                s.fund_fee_rate = f;
                if j == 0 {
                    bases.push(pool.clone());
                }
                for dir in [true, false] {
                    for input in sizes {
                        cases.push(case(
                            format!(
                                "mode{mode}-enabled{enabled}-schedule{j}-dir{dir}-input{input}"
                            ),
                            &pool,
                            input,
                            dir,
                        ));
                    }
                }
            }
        }
    }
    for (tag, r0, r1) in [
        ("tiny", 1, 1),
        ("dust-skew", u64::MAX - 10000, 1),
        ("large-exact", (1u64 << 53) + 21, (1u64 << 53) + 33),
        ("raw-near-max", u64::MAX - 10000, u64::MAX - 10000),
    ] {
        for mode in 0..3 {
            for dir in [true, false] {
                let mut p = base.clone();
                p.raydium_cpmm.creator_fee_on = mode;
                reserves(&mut p, r0, r1);
                for input in ["1", "2", "9999", "10001", "100000000"] {
                    cases.push(case(
                        format!("{tag}-mode{mode}-dir{dir}-{input}"),
                        &p,
                        input,
                        dir,
                    ));
                }
            }
        }
    }
    for dir in [true, false] {
        let mut p = base.clone();
        // Exactly fills the input raw vault to u64::MAX, then one greater must fail.
        p.raydium_cpmm.raw_vault0 = u64::MAX - 10000;
        p.raydium_cpmm.raw_vault1 = u64::MAX - 10000;
        refresh(&mut p);
        cases.push(case(
            format!("raw-vault-exact-u64-max-{dir}"),
            &p,
            "10000",
            dir,
        ));
        cases.push(case(
            format!("raw-vault-one-over-u64-max-{dir}"),
            &p,
            "10001",
            dir,
        ));
        let mut empty = base.clone();
        let s = &mut empty.raydium_cpmm;
        s.protocol_fees0 = 0;
        s.protocol_fees1 = 0;
        s.fund_fees0 = 0;
        s.fund_fees1 = 0;
        s.creator_fees0 = 0;
        s.creator_fees1 = 0;
        refresh(&mut empty);
        cases.push(case(
            format!("zero-accrued-ledgers-{dir}"),
            &empty,
            "1000003",
            dir,
        ));
    }
    // Atomic full-state rejection and administrative config boundaries.
    // The combined-rate equality case is a broader arithmetic probe outside public
    // configuration admission; its outcome is branch-specific, not always rejection.
    let invalids: Vec<(&str, Box<dyn Fn(&mut Pool)>)> = vec![
        (
            "creator-mode3",
            Box::new(|p| p.raydium_cpmm.creator_fee_on = 3),
        ),
        (
            "creator-mode256",
            Box::new(|p| p.raydium_cpmm.creator_fee_on = 256),
        ),
        (
            "combined-rate-equals-denominator-branch-dependent",
            Box::new(|p| p.fee_numerator = 999500),
        ),
        (
            "protocol-fund-over-denominator",
            Box::new(|p| p.raydium_cpmm.protocol_fee_rate = 1000000),
        ),
        (
            "share-over-denominator",
            Box::new(|p| p.raydium_cpmm.creator_fee_share_rate = 1000001),
        ),
        (
            "fee-rate-add-overflow",
            Box::new(|p| p.fee_numerator = u64::MAX),
        ),
        (
            "protocol-rate-add-overflow",
            Box::new(|p| p.raydium_cpmm.protocol_fee_rate = u64::MAX),
        ),
        (
            "ledger-sum-overflow",
            Box::new(|p| p.raydium_cpmm.protocol_fees0 = u64::MAX),
        ),
        (
            "ledger-exceeds-vault",
            Box::new(|p| p.raydium_cpmm.raw_vault0 = 100),
        ),
        ("zero-net-reserve", Box::new(|p| reserves(p, 0, 1000000))),
        ("reserve-mismatch", Box::new(|p| p.reserve0 = "7".into())),
        (
            "not-open",
            Box::new(|p| p.raydium_cpmm.open_time = p.raydium_cpmm.bank_unix_timestamp + 1),
        ),
    ];
    for (tag, change) in invalids {
        let mut p = base.clone();
        change(&mut p);
        for dir in [true, false] {
            cases.push(case(format!("invalid-{tag}-{dir}"), &p, "100000", dir));
        }
    }
    // share only changes collection: fees and all post balances remain identical.
    for share in [0, 1, 999999, 1000000] {
        for dir in [true, false] {
            let mut p = base.clone();
            p.raydium_cpmm.creator_fee_share_rate = share;
            cases.push(case(
                format!("share-collect-only-{share}-{dir}"),
                &p,
                "1000033",
                dir,
            ));
        }
    }
    // Preserve the explicit regression where ceil fee removes 1 before the curve.
    for dir in [true, false] {
        let mut p = base.clone();
        p.raydium_cpmm.creator_fee_enabled = false;
        reserves(&mut p, 1000, 1000);
        cases.push(case(
            format!("ceil-trade-small-input-10-dir{dir}"),
            &p,
            "10",
            dir,
        ));
    }
    for dir in [true, false] {
        let mut p = base.clone();
        p.fee_numerator = 0;
        p.raydium_cpmm.creator_fee_rate = 0;
        p.raydium_cpmm.creator_fee_on = if dir { 2 } else { 1 };
        reserves(&mut p, 1000, 1000);
        cases.push(case(
            format!("zero-fees-output-branch-input10-dir{dir}"),
            &p,
            "10",
            dir,
        ));
        p.raydium_cpmm.creator_fee_on = 0;
        cases.push(case(
            format!("zero-fees-input-branch-input10-dir{dir}"),
            &p,
            "10",
            dir,
        ));
    }
    let mut sequences = Vec::new();
    for mode in 0..3 {
        for enabled in [true, false] {
            for pattern in ["same-zero", "same-one", "mixed"] {
                let mut p = base.clone();
                p.raydium_cpmm.creator_fee_on = mode;
                p.raydium_cpmm.creator_fee_enabled = enabled;
                let initial = p.clone();
                let mut steps = Vec::new();
                for i in 0..8 {
                    let dir = pattern == "same-zero" || (pattern == "mixed" && i % 2 == 0);
                    let input = (1000003 + i * 71337).to_string();
                    let c = case(format!("step{i}"), &p, &input, dir);
                    let q = quote(&p, &input, dir).expect("sequence must succeed");
                    steps.push(c);
                    p = q.pool;
                }
                sequences.push(json!({"name":format!("mode{mode}-enabled{enabled}-{pattern}"),"pool":initial,"steps":steps,"expected_final_pool":p}));
            }
        }
    }
    json!({"schema_version":1,"reference":{"repository":"raydium-io/raydium-cp-swap","revision":REVISION,"execution":"native unchanged program crate, no-entrypoint; original curve and PoolState helpers; checked standard-token balance wrapper","synthetic_proof_identity":true,"full_sbf_transaction_executed":false},"base_pool":base,"base_pools":bases,"cases":cases,"sequences":sequences,"core_cases":core_boundaries(),"helper_cases":helper_boundaries()})
}

pub fn helper_boundaries() -> Value {
    let mut rows = Vec::new();
    for (name, which) in [
        ("protocol-ledger-u64-overflow", 0),
        ("fund-ledger-u64-overflow", 1),
        ("creator-ledger-u64-overflow", 2),
        ("disabled-nonzero-creator-fee", 3),
    ] {
        let mut n = PoolState::default();
        n.creator_fee_on = 0;
        n.enable_creator_fee = true;
        match which {
            0 => n.protocol_fees_token_0 = u64::MAX,
            1 => n.fund_fees_token_0 = u64::MAX,
            2 => n.creator_fees_token_0 = u64::MAX,
            _ => n.enable_creator_fee = false,
        }
        let p0 = n.protocol_fees_token_0;
        let f0 = n.fund_fees_token_0;
        let c0 = n.creator_fees_token_0;
        let initial = json!({"protocol_fees0":p0,"fund_fees0":f0,"creator_fees0":c0,"creator_fee_on":0,"creator_fee_enabled":n.enable_creator_fee,"other_fields":"PoolState::default()"});
        let result = catch_unwind(AssertUnwindSafe(|| {
            n.update_fees(1, 1, 1, TradeDirection::ZeroForOne)
        }));
        let reverted = !matches!(result, Ok(Ok(())));
        assert!(reverted);
        rows.push(json!({"name":name,"helper":"PoolState::update_fees","initial":initial,"args":{"protocol_fee":1,"fund_fee":1,"creator_fee":1,"zero_for_one":true},"expected":{"reverted":true},"rollback":"partial mutations discarded by wrapper; direct helper may mutate before panic"}));
    }
    let mut n = PoolState::default();
    n.enable_creator_fee = false;
    n.creator_fees_token_0 = 10;
    n.protocol_fees_token_0 = 20;
    n.fund_fees_token_0 = 30;
    let (r0, r1) = n.vault_amount_without_fee(100, 200).unwrap();
    rows.push(json!({"name":"disabled-creator-subtracts-accrual","helper":"PoolState::vault_amount_without_fee","expected":{"reserve0":r0.to_string(),"reserve1":r1.to_string()}}));
    for share in [0, 1, 250000, 1000000] {
        let mut pool = PoolState::default();
        pool.creator_fees_token_0 = 1001;
        pool.creator_fees_token_1 = 9;
        pool.protocol_fees_token_0 = 101;
        pool.protocol_fees_token_1 = 103;
        let (owed0, owed1) = pool.settle_creator_fee(share).unwrap();
        let p0 = pool.protocol_fees_token_0;
        let p1 = pool.protocol_fees_token_1;
        let c0 = pool.creator_fees_token_0;
        let c1 = pool.creator_fees_token_1;
        rows.push(json!({"name":format!("collect-share-{share}"),"helper":"PoolState::settle_creator_fee","initial":{"creator_fees0":1001,"creator_fees1":9,"protocol_fees0":101,"protocol_fees1":103,"other_fields":"PoolState::default()"},"args":{"share_rate":share},"expected":{"creator_owed0":owed0,"creator_owed1":owed1,"protocol_fees0":p0,"protocol_fees1":p1,"creator_fees0":c0,"creator_fees1":c1}}));
    }
    for which in 0..3 {
        let mut n = PoolState::default();
        n.enable_creator_fee = true;
        match which {
            0 => n.protocol_fees_token_0 = u64::MAX - 1,
            1 => n.fund_fees_token_0 = u64::MAX - 1,
            _ => n.creator_fees_token_0 = u64::MAX - 1,
        }
        n.update_fees(1, 1, 1, TradeDirection::ZeroForOne).unwrap();
        let p0 = n.protocol_fees_token_0;
        let f0 = n.fund_fees_token_0;
        let c0 = n.creator_fees_token_0;
        rows.push(json!({"name":format!("ledger-exact-u64-max-{which}"),"helper":"PoolState::update_fees","initial":{"max_minus_one_ledger_index":which,"other_fields":"PoolState::default() with creator enabled"},"args":{"protocol_fee":1,"fund_fee":1,"creator_fee":1,"zero_for_one":true},"expected":{"protocol_fees0":p0,"fund_fees0":f0,"creator_fees0":c0},"domain":"helper-only; unbacked ledger need not describe a reachable pool"}));
    }
    Value::Array(rows)
}

pub fn fixture() -> Value {
    let p = base_pool();
    let s = &p.raydium_cpmm;
    let program = pk(PROGRAM).unwrap();
    let pool = pk(&p.pool_id).unwrap();
    let (config, bump) = pda(&[b"amm_config", &0u16.to_be_bytes()], &program);
    let lp = pda(&[b"pool_lp_mint", pool.as_ref()], &program).0;
    let observation = pda(&[b"observation", pool.as_ref()], &program).0;
    json!({"schema_version":1,"pool":p,"pool_config":{"id":p.pool_id,"protocol":PROFILE,"fee_numerator":p.fee_numerator,"fee_denominator":p.fee_denominator,"program_id":PROGRAM,"token0":p.token0,"token1":p.token1,"accounts":[p.pool_id,s.vault0,s.vault1,s.amm_config,p.token0,p.token1],"raydium_cpmm":{"expected_programdata":s.programdata,"expected_program_code_sha256":s.program_code_sha256,"expected_deployed_slot":s.deployed_slot,"expected_token_program_code_sha256":s.token_program_code_sha256}},"metadata":{"synthetic_proof_identity":true,"program_code_utf8":String::from_utf8_lossy(SYNTHETIC_PROGRAM_CODE),"token_program_code_utf8":String::from_utf8_lossy(SYNTHETIC_TOKEN_CODE),"program_code_sha256":s.program_code_sha256,"token_program_code_sha256":s.token_program_code_sha256,"token_program":TOKEN_PROGRAM,"upgradeable_loader":"BPFLoaderUpgradeab1e11111111111111111111111","immutable_loader":"BPFLoader2111111111111111111111111111111111","clock":"SysvarC1ock11111111111111111111111111111111","clock_owner":"Sysvar1111111111111111111111111111111111111","pool_creator":key("xone:raydium:creator").to_string(),"lp_mint":lp.to_string(),"observation":observation.to_string(),"config_index":0,"config_bump":bump,"config":config.to_string(),"pool_len":PoolState::LEN,"config_len":AmmConfig::LEN,"pool_mint_keys_sorted_by_bytes":true,"authority_derivation":"official Anchor reexported solana_pubkey::Pubkey::find_program_address([AUTH_SEED], program)","bank_commitment":"finalized"}})
}
