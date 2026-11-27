//! Zero-liquidity DecreaseLiquidityV2 settlement for an existing position.
use super::{as_u128, update_rewards};
use num_bigint::BigUint;
use xone_domain::{
    err,
    raydium_clmm::{RaydiumCLMMState, RaydiumCLMMTick},
    Amount, Result,
};

const DECREASE_DISABLED: u8 = 1 << 1;
const COLLECT_FEE_DISABLED: u8 = 1 << 2;
const COLLECT_REWARD_DISABLED: u8 = 1 << 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Position {
    pub liquidity: u128,
    pub lower_tick: i32,
    pub upper_tick: i32,
    pub fees: [PositionAccrual; 2],
    pub rewards: [PositionAccrual; 3],
    pub recent_epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionAccrual {
    pub growth_inside_last_x64: u128,
    pub owed: u64,
}

pub struct Settlement {
    pub fees: [u64; 2],
    pub rewards: [u64; 3],
    pub next_position: Position,
    pub next_pool: RaydiumCLMMState,
}

fn growth_inside(global: u128, lower: u128, upper: u128, below: bool, above: bool) -> u128 {
    let lower = if below {
        global.wrapping_sub(lower)
    } else {
        lower
    };
    let upper = if above {
        global.wrapping_sub(upper)
    } else {
        upper
    };
    global.wrapping_sub(lower).wrapping_sub(upper)
}

fn accrue(entry: &mut PositionAccrual, growth: u128, liquidity: u128, reward: bool) -> Result<()> {
    let delta = growth.wrapping_sub(entry.growth_inside_last_x64);
    let scaled: BigUint = (BigUint::from(delta) * liquidity) >> 64;
    let amount = match u128::try_from(scaled) {
        Ok(amount) if amount < u128::from(u64::MAX) => amount as u64,
        Ok(_) => 0,
        Err(_) if reward => 0,
        Err(_) => return Err(err("RAYCLMM_POSITION_FEE_OVERFLOW")),
    };
    entry.owed = entry
        .owed
        .checked_add(amount)
        .ok_or_else(|| err("RAYCLMM_POSITION_OWED_OVERFLOW"))?;
    entry.growth_inside_last_x64 = growth;
    Ok(())
}

pub fn settle(
    state: &RaydiumCLMMState,
    position: &Position,
    lower: &RaydiumCLMMTick,
    upper: &RaydiumCLMMTick,
    status: u8,
    balances: [u64; 3],
) -> Result<Settlement> {
    let disabled = DECREASE_DISABLED | COLLECT_FEE_DISABLED | COLLECT_REWARD_DISABLED;
    if status & disabled == disabled {
        return Err(err("RAYCLMM_POSITION_NOT_APPROVED"));
    }
    if lower.tick != position.lower_tick
        || upper.tick != position.upper_tick
        || lower.tick >= upper.tick
        || lower.liquidity_gross.0 < BigUint::from(position.liquidity)
        || upper.liquidity_gross.0 < BigUint::from(position.liquidity)
        || state.reward_infos.len() != 3
        || lower.reward_growths_outside.len() != 3
        || upper.reward_growths_outside.len() != 3
    {
        return Err(err("RAYCLMM_POSITION_TICKS"));
    }
    let mut result = Settlement {
        fees: [0; 2],
        rewards: [0; 3],
        next_position: position.clone(),
        next_pool: state.clone(),
    };
    if status & DECREASE_DISABLED == 0 {
        update_rewards(&mut result.next_pool)?;
        let inside = |global: &Amount, low: &Amount, high: &Amount| -> Result<u128> {
            Ok(growth_inside(
                as_u128(global.0.clone())?,
                as_u128(low.0.clone())?,
                as_u128(high.0.clone())?,
                state.tick_current < lower.tick,
                state.tick_current >= upper.tick,
            ))
        };
        let growths = [
            inside(
                &state.fee_growth_global0,
                &lower.fee_growth_outside0,
                &upper.fee_growth_outside0,
            )?,
            inside(
                &state.fee_growth_global1,
                &lower.fee_growth_outside1,
                &upper.fee_growth_outside1,
            )?,
        ];
        for (entry, growth) in result.next_position.fees.iter_mut().zip(growths) {
            accrue(entry, growth, position.liquidity, false)?;
        }
        for (index, reward) in result.next_pool.reward_infos.iter().enumerate() {
            let growth = if reward.initialized() {
                inside(
                    &reward.reward_growth_global_x64,
                    &lower.reward_growths_outside[index],
                    &upper.reward_growths_outside[index],
                )?
            } else {
                0
            };
            accrue(
                &mut result.next_position.rewards[index],
                growth,
                position.liquidity,
                true,
            )?;
        }
        result.next_position.recent_epoch = state.bank_epoch;
    }
    if status & COLLECT_FEE_DISABLED == 0 {
        for (amount, entry) in result.fees.iter_mut().zip(&mut result.next_position.fees) {
            *amount = std::mem::take(&mut entry.owed);
        }
    }
    if status & COLLECT_REWARD_DISABLED == 0 {
        for (index, reward) in result.next_pool.reward_infos.iter_mut().enumerate() {
            if !reward.initialized() {
                continue;
            }
            let entry = &mut result.next_position.rewards[index];
            if entry.owed
                > reward
                    .reward_total_emitted
                    .checked_sub(reward.reward_claimed)
                    .ok_or_else(|| err("RAYCLMM_POSITION_REWARD_LEDGER"))?
            {
                return Err(err("RAYCLMM_POSITION_REWARD_LEDGER"));
            }
            let amount = balances[index].min(entry.owed);
            reward.reward_claimed += amount;
            entry.owed -= amount;
            result.rewards[index] = amount;
        }
    }
    Ok(result)
}
