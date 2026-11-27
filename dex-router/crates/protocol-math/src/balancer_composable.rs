//! ComposableStable math at official Balancer revision
//! e91a2b643a49856f51a648d175667c1b48cf3377 (separately pinned from Weighted).
//! Regular swap path drops BPT before the N-token StableMath invariant/solve.
//! Supply accounting at the current cached rates is shared by the supply view
//! and the pre-join/exit fee calculation.
use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::{err, Result};
const ONE: u64 = 1_000_000_000_000_000_000;
const PRECISION: u32 = 1000;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposableQuote {
    pub amount_out: BigUint,
    pub next_balances: Vec<BigUint>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BptMathQuote {
    pub amount_out: BigUint,
    pub next_underlying_balances: Vec<BigUint>,
    /// Supply after the single-token operation, on a supplied post-fee basis.
    pub next_actual_supply: BigUint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedRate {
    pub current: BigUint,
    pub old: BigUint,
}

pub enum YieldFeePolicy<'a> {
    /// Per-token exemptions at the pinned ComposableStable source revision.
    PerToken(&'a [bool]),
    /// Released V5 and V6 use a pool-wide exemption and bound invariant growth.
    PoolWide { exempt: bool },
}

pub struct SupplyInput<'a> {
    /// Raw Vault balances and effective scaling factors, in registered order.
    pub balances: &'a [BigUint],
    pub scales: &'a [BigUint],
    pub bpt_index: usize,
    /// None for BPT and for tokens without a rate provider.
    pub rates: &'a [Option<CachedRate>],
    pub yield_fee_policy: YieldFeePolicy<'a>,
    pub total_supply: &'a BigUint,
    pub last_join_exit_amp: &'a BigUint,
    pub last_post_join_exit_invariant: &'a BigUint,
    pub protocol_swap_fee: &'a BigUint,
    pub protocol_yield_fee: &'a BigUint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupplyView {
    pub virtual_supply: BigUint,
    pub pending_protocol_bpt: BigUint,
    pub actual_supply: BigUint,
    pub invariant_at_last_join_exit_amp: BigUint,
}

#[derive(Clone, Copy)]
pub enum BptSwapKind {
    GivenIn,
    GivenOut,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BptSwapQuote {
    pub amount_in: BigUint,
    pub amount_out: BigUint,
    pub next_balances: Vec<BigUint>,
    pub next_total_supply: BigUint,
    pub next_actual_supply_after_mint: BigUint,
    pub protocol_bpt_before_swap: BigUint,
    pub protocol_bpt_after_swap: BigUint,
    pub last_join_exit_invariant: BigUint,
    pub last_join_exit_amp: BigUint,
    pub next_rates: Vec<Option<CachedRate>>,
}
fn checked(x: BigUint) -> Result<BigUint> {
    if x.bits() > 256 {
        Err(err("BALANCER_COMPOSABLE_UINT256"))
    } else {
        Ok(x)
    }
}
fn add(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    checked(a + b)
}
fn mul(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    checked(a * b)
}
fn sub(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    if a < b {
        Err(err("BALANCER_COMPOSABLE_UNDERFLOW"))
    } else {
        Ok(a - b)
    }
}
fn down(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    if b.is_zero() {
        Err(err("BALANCER_COMPOSABLE_ZERO_DIVISION"))
    } else {
        Ok(a / b)
    }
}
fn up(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    if b.is_zero() {
        Err(err("BALANCER_COMPOSABLE_ZERO_DIVISION"))
    } else if a.is_zero() {
        Ok(a.clone())
    } else {
        Ok((a - BigUint::one()) / b + BigUint::one())
    }
}
fn md(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    down(&mul(a, b)?, &BigUint::from(ONE))
}
fn mu(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    up(&mul(a, b)?, &BigUint::from(ONE))
}
fn dd(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    down(&mul(a, &BigUint::from(ONE))?, b)
}
fn du(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    up(&mul(a, &BigUint::from(ONE))?, b)
}
fn complement(x: &BigUint) -> BigUint {
    let one = BigUint::from(ONE);
    if x >= &one {
        BigUint::zero()
    } else {
        one - x
    }
}
fn protocol_bpt(supply: &BigUint, ownership: &BigUint) -> Result<BigUint> {
    down(&mul(supply, ownership)?, &complement(ownership))
}
fn sum(xs: &[BigUint]) -> Result<BigUint> {
    xs.iter().try_fold(BigUint::zero(), |a, b| add(&a, b))
}
fn near(a: &BigUint, b: &BigUint) -> bool {
    if a > b {
        a - b <= BigUint::one()
    } else {
        b - a <= BigUint::one()
    }
}
fn validate(
    b: &[BigUint],
    amp: &BigUint,
    scales: &[BigUint],
    fee: &BigUint,
    input: &BigUint,
) -> Result<()> {
    if !(2..=5).contains(&b.len()) || b.len() != scales.len() {
        return Err(err("BALANCER_COMPOSABLE_LENGTH"));
    }
    for x in [amp, fee, input] {
        checked(x.clone())?;
    }
    if amp < &BigUint::from(1000u32) || amp > &BigUint::from(5_000_000u32) {
        return Err(err("BALANCER_COMPOSABLE_AMP"));
    }
    if fee > &BigUint::from(ONE) {
        return Err(err("BALANCER_COMPOSABLE_FEE"));
    }
    if input.is_zero() {
        return Err(err("ZERO_INPUT"));
    }
    for (balance, scale) in b.iter().zip(scales) {
        if balance.bits() > 112 {
            return Err(err("BALANCER_COMPOSABLE_UINT112"));
        }
        if balance.is_zero() {
            return Err(err("INSUFFICIENT_LIQUIDITY"));
        }
        checked(scale.clone())?;
        // Effective decimal/rate factors are caller-observed, not assumed powers of ten.
        if scale.is_zero() {
            return Err(err("BALANCER_COMPOSABLE_SCALE"));
        }
    }
    Ok(())
}
fn invariant(amp: &BigUint, b: &[BigUint]) -> Result<BigUint> {
    let n = BigUint::from(b.len());
    let p = BigUint::from(PRECISION);
    let s = sum(b)?;
    if s.is_zero() {
        return Ok(s);
    }
    let ann = mul(amp, &n)?;
    let mut d = s.clone();
    for _ in 0..255 {
        let mut dp = d.clone();
        for x in b {
            dp = down(&mul(&dp, &d)?, &mul(x, &n)?)?;
        }
        let previous = d;
        let numerator = mul(
            &add(&down(&mul(&ann, &s)?, &p)?, &mul(&dp, &n)?)?,
            &previous,
        )?;
        let denominator = add(
            &down(&mul(&sub(&ann, &p)?, &previous)?, &p)?,
            &mul(&(&n + 1u8), &dp)?,
        )?;
        d = down(&numerator, &denominator)?;
        if near(&d, &previous) {
            return Ok(d);
        }
    }
    Err(err("BALANCER_COMPOSABLE_INVARIANT_CONVERGENCE"))
}

/// The pool's getActualSupply reads the stored caches, including expired caches.
/// A swap callback must refresh the applicable caches before calculating this view.
pub fn supply_view(input: &SupplyInput<'_>) -> Result<SupplyView> {
    let SupplyInput {
        balances,
        scales,
        bpt_index,
        rates,
        ref yield_fee_policy,
        total_supply,
        last_join_exit_amp,
        last_post_join_exit_invariant,
        protocol_swap_fee,
        protocol_yield_fee,
    } = *input;
    let one = BigUint::from(ONE);
    if !(3..=6).contains(&balances.len())
        || scales.len() != balances.len()
        || rates.len() != balances.len()
        || bpt_index >= balances.len()
    {
        return Err(err("BALANCER_COMPOSABLE_LENGTH"));
    }
    if scales[bpt_index] != one || rates[bpt_index].is_some() {
        return Err(err("BALANCER_COMPOSABLE_BPT_STATE"));
    }
    if balances.iter().any(|balance| balance.bits() > 112) {
        return Err(err("BALANCER_COMPOSABLE_UINT112"));
    }
    if !(BigUint::from(PRECISION)..=BigUint::from(5_000_000u32)).contains(last_join_exit_amp)
        || last_post_join_exit_invariant.bits() > 233
        || protocol_swap_fee > &one
        || protocol_yield_fee > &one
    {
        return Err(err("BALANCER_COMPOSABLE_FEE_STATE"));
    }
    let virtual_supply = sub(&checked(total_supply.clone())?, &balances[bpt_index])?;
    if rates
        .iter()
        .flatten()
        .any(|rate| rate.current.bits() > 96 || rate.old.bits() > 96)
    {
        return Err(err("BALANCER_COMPOSABLE_RATE_WIDTH"));
    }
    if let YieldFeePolicy::PerToken(exempt) = yield_fee_policy {
        if exempt.len() != rates.len()
            || exempt
                .iter()
                .zip(rates)
                .any(|(&exempt, rate)| exempt && rate.is_none())
        {
            return Err(err("BALANCER_COMPOSABLE_YIELD_EXEMPTION"));
        }
    }
    let scaled = balances
        .iter()
        .zip(scales)
        .enumerate()
        .filter(|(i, _)| *i != bpt_index)
        .map(|(_, (balance, scale))| md(balance, &checked(scale.clone())?))
        .collect::<Result<Vec<_>>>()?;
    let total_invariant = invariant(last_join_exit_amp, &scaled)?;
    let ownership = match yield_fee_policy {
        YieldFeePolicy::PoolWide { .. } if &total_invariant <= last_post_join_exit_invariant => {
            BigUint::zero()
        }
        _ => protocol_ownership(input, &scaled, &total_invariant)?,
    };
    let pending_protocol_bpt = protocol_bpt(&virtual_supply, &ownership)?;
    let actual_supply = add(&virtual_supply, &pending_protocol_bpt)?;
    Ok(SupplyView {
        virtual_supply,
        pending_protocol_bpt,
        actual_supply,
        invariant_at_last_join_exit_amp: total_invariant,
    })
}

fn protocol_ownership(
    input: &SupplyInput<'_>,
    scaled: &[BigUint],
    total: &BigUint,
) -> Result<BigUint> {
    let amp = input.last_join_exit_amp;
    let last = input.last_post_join_exit_invariant;
    let indices = (0..input.balances.len()).filter(|&i| i != input.bpt_index);
    let adjusted = indices
        .clone()
        .zip(scaled)
        .map(|(i, balance)| match &input.rates[i] {
            Some(rate) => down(&mul(balance, &rate.old)?, &rate.current),
            None => Ok(balance.clone()),
        })
        .collect::<Result<Vec<_>>>()?;
    let mut swap = if adjusted == scaled {
        total.clone()
    } else {
        invariant(amp, &adjusted)?
    };
    let non_exempt = match input.yield_fee_policy {
        YieldFeePolicy::PoolWide { exempt } => {
            swap = swap.min(total.clone()).max(last.clone());
            if exempt {
                swap.clone()
            } else {
                total.clone()
            }
        }
        YieldFeePolicy::PerToken(exempt) => {
            let balances = indices
                .zip(scaled.iter().zip(&adjusted))
                .map(|(i, (current, adjusted))| {
                    if exempt[i] {
                        adjusted.clone()
                    } else {
                        current.clone()
                    }
                })
                .collect::<Vec<_>>();
            if balances == scaled {
                total.clone()
            } else if balances == adjusted {
                swap.clone()
            } else {
                invariant(amp, &balances)?
            }
        }
    };
    let swap_growth = &swap - (&swap).min(last);
    let yield_growth = &non_exempt - (&non_exempt).min(&swap);
    let swap_fee = md(&dd(&swap_growth, total)?, input.protocol_swap_fee)?;
    let yield_fee = md(&dd(&yield_growth, total)?, input.protocol_yield_fee)?;
    add(&swap_fee, &yield_fee)
}
fn token_balance(amp: &BigUint, b: &[BigUint], d: &BigUint, index: usize) -> Result<BigUint> {
    let n = BigUint::from(b.len());
    let ann = mul(amp, &n)?;
    let precision = BigUint::from(PRECISION);
    // Preserve P_D's token order and each intermediate division.
    let mut pd = mul(&b[0], &n)?;
    for x in &b[1..] {
        pd = down(&mul(&mul(&pd, x)?, &n)?, d)?;
    }
    let s = sub(&sum(b)?, &b[index])?;
    let inv2 = mul(d, d)?;
    let c = mul(&mul(&up(&inv2, &mul(&ann, &pd)?)?, &precision)?, &b[index])?;
    let linear = add(&s, &mul(&down(d, &ann)?, &precision)?)?;
    let mut y = up(&add(&inv2, &c)?, &add(d, &linear)?)?;
    for _ in 0..255 {
        let prev = y;
        y = up(
            &add(&mul(&prev, &prev)?, &c)?,
            &sub(&add(&mul(&prev, &BigUint::from(2u8))?, &linear)?, d)?,
        )?;
        if near(&y, &prev) {
            return Ok(y);
        }
    }
    Err(err("BALANCER_COMPOSABLE_BALANCE_CONVERGENCE"))
}

/// Token-to-token exact input, full registered order INCLUDING BPT. Supports
/// 2..=5 underlying assets. Effective scales must match the callback AFTER
/// `_cacheTokenRatesIfNecessary`; the caller proves rate/cache and block binding.
/// Current amplification is scaled by 1000. `bpt_index` may be anywhere.
/// BPT input/output is deliberately rejected here. No supply/fee history is
/// required for regular swaps; every uint256/uint112 arithmetic bound is checked.
#[allow(clippy::too_many_arguments)]
pub fn quote_exact_in(
    balances: &[BigUint],
    amplification: &BigUint,
    scales: &[BigUint],
    fee: &BigUint,
    input: &BigUint,
    index_in: usize,
    index_out: usize,
    bpt_index: usize,
) -> Result<ComposableQuote> {
    if !(3..=6).contains(&balances.len()) || scales.len() != balances.len() {
        return Err(err("BALANCER_COMPOSABLE_LENGTH"));
    }
    if [index_in, index_out, bpt_index]
        .iter()
        .any(|&i| i >= balances.len())
        || index_in == index_out
    {
        return Err(err("BALANCER_COMPOSABLE_INDEX"));
    }
    if index_in == bpt_index || index_out == bpt_index {
        return Err(err("BALANCER_COMPOSABLE_BPT_REQUIRES_SUPPLY"));
    }
    if scales[bpt_index] != BigUint::from(ONE) || balances[bpt_index].bits() > 112 {
        return Err(err("BALANCER_COMPOSABLE_BPT_STATE"));
    }
    let underlying: Vec<_> = balances
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != bpt_index)
        .map(|(_, v)| v.clone())
        .collect();
    let factors: Vec<_> = scales
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != bpt_index)
        .map(|(_, v)| v.clone())
        .collect();
    validate(&underlying, amplification, &factors, fee, input)?;
    let i = index_in - usize::from(index_in > bpt_index);
    let j = index_out - usize::from(index_out > bpt_index);
    // BaseGeneralPool: raw fee first, then balance/input upscaling.
    let net = sub(input, &mu(input, fee)?)?;
    let mut b = underlying
        .iter()
        .zip(&factors)
        .map(|(v, s)| md(v, s))
        .collect::<Result<Vec<_>>>()?;
    let d = invariant(amplification, &b)?;
    b[i] = add(&b[i], &md(&net, &factors[i])?)?;
    let final_out = token_balance(amplification, &b, &d, j)?;
    let scaled_out = sub(&sub(&b[j], &final_out)?, &BigUint::one())?;
    let amount_out = dd(&scaled_out, &factors[j])?;
    if amount_out.is_zero() || amount_out >= balances[index_out] {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    let mut next_balances = balances.to_vec();
    next_balances[index_in] = add(&balances[index_in], input)?;
    if next_balances[index_in].bits() > 112 {
        return Err(err("BALANCER_COMPOSABLE_UINT112"));
    }
    next_balances[index_out] = sub(&balances[index_out], &amount_out)?;
    Ok(ComposableQuote {
        amount_out,
        next_balances,
    })
}

/// Single-token exact-input BPT math, using original StableMath fee allocation.
/// This is a mathematical kernel, NOT an executable Composable pool quote.
/// `balances`/`scales` EXCLUDE BPT; `actual_supply_after_fees` MUST already
/// include all pending protocol swap/yield fee BPT. `token_to_bpt=true` joins
/// with an underlying token, otherwise input is BPT and output is that token.
/// Does not enforce Vault BPT inventory, mint pending fees, update rate caches,
/// minimum BPT/token balances or lastJoinExitData. Integration must execute those
/// obligations before admitting BPT swaps. Underlying cash and math are checked.
#[allow(clippy::too_many_arguments)]
pub fn quote_bpt_exact_in_after_fees(
    balances: &[BigUint],
    amplification: &BigUint,
    scales: &[BigUint],
    fee: &BigUint,
    input: &BigUint,
    actual_supply_after_fees: &BigUint,
    token_index: usize,
    token_to_bpt: bool,
) -> Result<BptMathQuote> {
    validate(balances, amplification, scales, fee, input)?;
    if token_index >= balances.len() {
        return Err(err("BALANCER_COMPOSABLE_INDEX"));
    }
    checked(actual_supply_after_fees.clone())?;
    if actual_supply_after_fees.is_zero() {
        return Err(err("BALANCER_COMPOSABLE_SUPPLY"));
    }
    let b = balances
        .iter()
        .zip(scales)
        .map(|(v, s)| md(v, s))
        .collect::<Result<Vec<_>>>()?;
    let d = invariant(amplification, &b)?;
    let scaled_input = if token_to_bpt {
        md(input, &scales[token_index])?
    } else {
        input.clone()
    };
    let scaled_out = bpt_given_in(
        &b,
        amplification,
        fee,
        &scaled_input,
        actual_supply_after_fees,
        &d,
        token_index,
        token_to_bpt,
    )?;
    let mut next_underlying_balances = balances.to_vec();
    let (amount_out, next_actual_supply) = if token_to_bpt {
        next_underlying_balances[token_index] = add(&balances[token_index], input)?;
        if next_underlying_balances[token_index].bits() > 112 {
            return Err(err("BALANCER_COMPOSABLE_UINT112"));
        }
        let next = add(actual_supply_after_fees, &scaled_out)?;
        (scaled_out, next)
    } else {
        let amount_out = dd(&scaled_out, &scales[token_index])?;
        if amount_out >= balances[token_index] {
            return Err(err("INSUFFICIENT_OUTPUT"));
        }
        next_underlying_balances[token_index] = sub(&balances[token_index], &amount_out)?;
        (amount_out, sub(actual_supply_after_fees, input)?)
    };
    if amount_out.is_zero() {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    Ok(BptMathQuote {
        amount_out,
        next_underlying_balances,
        next_actual_supply,
    })
}

#[allow(clippy::too_many_arguments)]
fn bpt_given_in(
    b: &[BigUint],
    amplification: &BigUint,
    fee: &BigUint,
    input: &BigUint,
    supply: &BigUint,
    d: &BigUint,
    index: usize,
    token_to_bpt: bool,
) -> Result<BigUint> {
    let one = BigUint::from(ONE);
    let total = sum(b)?;
    if token_to_bpt {
        let mut ratios = Vec::with_capacity(b.len());
        let mut ratio_with_fees = BigUint::zero();
        for (k, x) in b.iter().enumerate() {
            let amount = if k == index {
                input.clone()
            } else {
                BigUint::zero()
            };
            let ratio = dd(&add(x, &amount)?, x)?;
            ratio_with_fees = add(&ratio_with_fees, &md(&ratio, &dd(x, &total)?)?)?;
            ratios.push(ratio);
        }
        let mut next = b.to_vec();
        for k in 0..next.len() {
            let amount = if k == index {
                input.clone()
            } else {
                BigUint::zero()
            };
            let net = if ratios[k] > ratio_with_fees {
                let non_taxable = md(&b[k], &sub(&ratio_with_fees, &one)?)?;
                let taxable = sub(&amount, &non_taxable)?;
                add(&non_taxable, &md(&taxable, &sub(&one, fee)?)?)?
            } else {
                amount
            };
            next[k] = add(&b[k], &net)?;
        }
        let ratio = dd(&invariant(amplification, &next)?, d)?;
        if ratio > one {
            md(supply, &(ratio - &one))
        } else {
            Ok(BigUint::zero())
        }
    } else {
        if input >= supply {
            return Err(err("BALANCER_COMPOSABLE_SUPPLY"));
        }
        let new_d = mu(&du(&sub(supply, input)?, supply)?, d)?;
        let y = token_balance(amplification, b, &new_d, index)?;
        let gross = sub(&b[index], &y)?;
        let taxable = mu(&gross, &complement(&dd(&b[index], &total)?))?;
        let non_taxable = sub(&gross, &taxable)?;
        add(&non_taxable, &md(&taxable, &sub(&one, fee)?)?)
    }
}

#[allow(clippy::too_many_arguments)]
fn bpt_given_out(
    balances: &[BigUint],
    amp: &BigUint,
    fee: &BigUint,
    output: &BigUint,
    supply: &BigUint,
    invariant_before: &BigUint,
    index: usize,
    token_to_bpt: bool,
) -> Result<BigUint> {
    let fee_complement = complement(fee);
    let total = sum(balances)?;
    if token_to_bpt {
        let ratio = du(&add(supply, output)?, supply)?;
        let target = mu(&ratio, invariant_before)?;
        let balance = &balances[index];
        let amount = sub(&token_balance(amp, balances, &target, index)?, balance)?;
        let taxable = mu(&amount, &complement(&dd(balance, &total)?))?;
        return add(&sub(&amount, &taxable)?, &du(&taxable, &fee_complement)?);
    }

    let mut ratios = Vec::with_capacity(balances.len());
    let mut weighted_ratio = BigUint::zero();
    let zero = BigUint::zero();
    for (i, balance) in balances.iter().enumerate() {
        let amount = if i == index { output } else { &zero };
        let ratio = du(&sub(balance, amount)?, balance)?;
        weighted_ratio = add(&weighted_ratio, &mu(&ratio, &du(balance, &total)?)?)?;
        ratios.push(ratio);
    }
    let mut next = Vec::with_capacity(balances.len());
    for (i, (balance, ratio)) in balances.iter().zip(ratios).enumerate() {
        let amount = if i == index { output } else { &zero };
        let gross = if weighted_ratio > ratio {
            let non_taxable = md(balance, &complement(&weighted_ratio))?;
            let taxable = sub(amount, &non_taxable)?;
            add(&non_taxable, &du(&taxable, &fee_complement)?)?
        } else {
            amount.clone()
        };
        next.push(sub(balance, &gross)?);
    }
    let ratio = dd(&invariant(amp, &next)?, invariant_before)?;
    mu(supply, &complement(&ratio))
}

/// Models the BPT swap callback and Vault balance deltas. Supply inputs must
/// reflect the token-rate caches and effective factors at callback entry.
pub fn quote_bpt_swap(
    input: &SupplyInput<'_>,
    amp: &BigUint,
    fee: &BigUint,
    amount: &BigUint,
    index_in: usize,
    index_out: usize,
    kind: BptSwapKind,
) -> Result<BptSwapQuote> {
    let bpt = input.bpt_index;
    if index_in >= input.balances.len()
        || index_out >= input.balances.len()
        || index_in == index_out
        || (index_in != bpt && index_out != bpt)
    {
        return Err(err("BALANCER_COMPOSABLE_INDEX"));
    }
    let supply = supply_view(input)?;
    if supply.actual_supply.is_zero() {
        return Err(err("BALANCER_COMPOSABLE_SUPPLY"));
    }
    let token_to_bpt = index_out == bpt;
    let token = if token_to_bpt { index_in } else { index_out };
    let index = token - usize::from(token > bpt);
    let underlying: Vec<_> = (0..input.balances.len()).filter(|&i| i != bpt).collect();
    let raw: Vec<_> = underlying
        .iter()
        .map(|&i| input.balances[i].clone())
        .collect();
    let scales: Vec<_> = underlying
        .iter()
        .map(|&i| input.scales[i].clone())
        .collect();
    validate(&raw, amp, &scales, fee, amount)?;
    let mut balances = raw
        .iter()
        .zip(&scales)
        .map(|(v, s)| md(v, s))
        .collect::<Result<Vec<_>>>()?;
    let invariant_before = if amp == input.last_join_exit_amp {
        supply.invariant_at_last_join_exit_amp
    } else {
        invariant(amp, &balances)?
    };
    let (amount_in, amount_out, token_delta) = match kind {
        BptSwapKind::GivenIn => {
            let known = md(amount, &input.scales[index_in])?;
            let calculated = bpt_given_in(
                &balances,
                amp,
                fee,
                &known,
                &supply.actual_supply,
                &invariant_before,
                index,
                token_to_bpt,
            )?;
            let output = dd(&calculated, &input.scales[index_out])?;
            let delta = if token_to_bpt { known } else { calculated };
            (amount.clone(), output, delta)
        }
        BptSwapKind::GivenOut => {
            if amount > &input.balances[index_out] {
                return Err(err("INSUFFICIENT_LIQUIDITY"));
            }
            let known = md(amount, &input.scales[index_out])?;
            let calculated = bpt_given_out(
                &balances,
                amp,
                fee,
                &known,
                &supply.actual_supply,
                &invariant_before,
                index,
                token_to_bpt,
            )?;
            let input_amount = du(&calculated, &input.scales[index_in])?;
            let delta = if token_to_bpt { calculated } else { known };
            (input_amount, amount.clone(), delta)
        }
    };
    if amount_in.is_zero() || amount_out.is_zero() {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    let mut next_balances = input.balances.to_vec();
    next_balances[index_in] = add(&next_balances[index_in], &amount_in)?;
    next_balances[index_out] = sub(&next_balances[index_out], &amount_out)?;
    if next_balances[index_in].bits() > 112 {
        return Err(err("BALANCER_COMPOSABLE_UINT112"));
    }
    let supply_before_mint = if token_to_bpt {
        balances[index] = add(&balances[index], &token_delta)?;
        add(&supply.actual_supply, &amount_out)?
    } else {
        balances[index] = sub(&balances[index], &token_delta)?;
        sub(&supply.actual_supply, &amount_in)?
    };
    // The callback stores the invariant of the in-memory scaled balances.
    // The Vault independently settles the downscaled token delta above.
    let invariant_after = invariant(amp, &balances)?;
    if invariant_after.bits() > 233 {
        return Err(err("BALANCER_COMPOSABLE_FEE_STATE"));
    }
    let ratio = dd(&supply_before_mint, &supply.actual_supply)?;
    let feeless = md(&invariant_before, &ratio)?;
    let protocol_bpt_after_swap = if invariant_after > feeless {
        let growth = sub(&invariant_after, &feeless)?;
        let ownership = down(&mul(&growth, input.protocol_swap_fee)?, &invariant_after)?;
        protocol_bpt(&supply_before_mint, &ownership)?
    } else {
        BigUint::zero()
    };
    let next_total_supply = add(
        &add(input.total_supply, &supply.pending_protocol_bpt)?,
        &protocol_bpt_after_swap,
    )?;
    let next_rates = input
        .rates
        .iter()
        .map(|rate| {
            rate.as_ref().map(|rate| CachedRate {
                old: rate.current.clone(),
                current: rate.current.clone(),
            })
        })
        .collect();
    Ok(BptSwapQuote {
        amount_in,
        amount_out,
        next_balances,
        next_total_supply,
        next_actual_supply_after_mint: add(&supply_before_mint, &protocol_bpt_after_swap)?,
        protocol_bpt_before_swap: supply.pending_protocol_bpt,
        protocol_bpt_after_swap,
        last_join_exit_invariant: invariant_after,
        last_join_exit_amp: amp.clone(),
        next_rates,
    })
}

/// Regular-token GIVEN_OUT; BPT supply/fee paths remain separate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposableExactOutQuote {
    pub amount_in: BigUint,
    pub next_balances: Vec<BigUint>,
}
#[allow(clippy::too_many_arguments)]
pub fn quote_exact_out(
    balances: &[BigUint],
    amplification: &BigUint,
    scales: &[BigUint],
    fee: &BigUint,
    output: &BigUint,
    index_in: usize,
    index_out: usize,
    bpt_index: usize,
) -> Result<ComposableExactOutQuote> {
    if !(3..=6).contains(&balances.len()) || scales.len() != balances.len() {
        return Err(err("BALANCER_COMPOSABLE_LENGTH"));
    }
    if [index_in, index_out, bpt_index]
        .iter()
        .any(|&i| i >= balances.len())
        || index_in == index_out
    {
        return Err(err("BALANCER_COMPOSABLE_INDEX"));
    }
    if index_in == bpt_index || index_out == bpt_index {
        return Err(err("BALANCER_COMPOSABLE_BPT_REQUIRES_SUPPLY"));
    }
    if scales[bpt_index] != BigUint::from(ONE) || balances[bpt_index].bits() > 112 {
        return Err(err("BALANCER_COMPOSABLE_BPT_STATE"));
    }
    let underlying: Vec<_> = balances
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != bpt_index)
        .map(|(_, v)| v.clone())
        .collect();
    let factors: Vec<_> = scales
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != bpt_index)
        .map(|(_, v)| v.clone())
        .collect();
    if output.is_zero() {
        return Err(err("EXACT_OUT_ZERO_OUTPUT"));
    }
    validate(&underlying, amplification, &factors, fee, output)?;
    if fee >= &BigUint::from(ONE) {
        return Err(err("BALANCER_COMPOSABLE_FEE"));
    }
    if output >= &balances[index_out] {
        return Err(err("INSUFFICIENT_LIQUIDITY"));
    }
    let i = index_in - usize::from(index_in > bpt_index);
    let j = index_out - usize::from(index_out > bpt_index);
    let mut b = underlying
        .iter()
        .zip(&factors)
        .map(|(v, s)| md(v, s))
        .collect::<Result<Vec<_>>>()?;
    let d = invariant(amplification, &b)?;
    b[j] = sub(&b[j], &md(output, &factors[j])?)?;
    let final_in = token_balance(amplification, &b, &d, i)?;
    let scaled_in = add(&sub(&final_in, &b[i])?, &BigUint::one())?;
    let raw_in = du(&scaled_in, &factors[i])?;
    let amount_in = du(&raw_in, &sub(&BigUint::from(ONE), fee)?)?;
    if amount_in.is_zero() {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    let mut next_balances = balances.to_vec();
    next_balances[index_in] = add(&balances[index_in], &amount_in)?;
    if next_balances[index_in].bits() > 112 {
        return Err(err("BALANCER_COMPOSABLE_UINT112"));
    }
    next_balances[index_out] = sub(&balances[index_out], output)?;
    Ok(ComposableExactOutQuote {
        amount_in,
        next_balances,
    })
}
