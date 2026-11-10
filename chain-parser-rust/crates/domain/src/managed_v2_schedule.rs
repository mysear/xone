//! Raw 20230411 ManagedPoolStorageLib / ManagedPoolTokenStorageLib schedule.
use crate::{Amount, Result};
use serde::{Deserialize, Serialize};
pub const PROTOCOL: &str = "balancer-v2-managed-scheduled";
pub const ADAPTER_VERSION: &str = "balancer-v2/20230411-managed-scheduled-1";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagedV2ScheduledToken {
    pub token: String,
    pub state_word: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagedV2Schedule {
    pub pool_state_word: Amount,
    pub tokens: Vec<ManagedV2ScheduledToken>,
}
/// Computed execution view; never part of the parent's canonical source hash.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagedV2ScheduleView {
    pub weights: Vec<Amount>,
    pub scaling: Vec<Amount>,
    pub swap_fee: Amount,
    pub weight_start: u64,
    pub weight_end: u64,
    pub fee_start: u64,
    pub fee_end: u64,
    pub swaps_enabled: bool,
    pub must_allowlist_lps: bool,
    pub recovery_mode: bool,
    pub join_exit_enabled: bool,
}

use crate::{err, protocols::validate_address};
use num_bigint::BigUint;
const ONE: u64 = 1_000_000_000_000_000_000;
const MIN_WEIGHT: u64 = 10_000_000_000_000_000;
const MIN_FEE: u64 = 1_000_000_000_000;
const MAX_FEE: u64 = 950_000_000_000_000_000;

// All field widths are at most 64. Callers validate the whole word before slicing
// so a directly constructed Amount cannot hide bits above the original bytes32.
fn field(word: &Amount, offset: usize, width: usize) -> u64 {
    let value = (&word.0 >> offset) & ((BigUint::from(1u8) << width) - BigUint::from(1u8));
    value.to_u64_digits().first().copied().unwrap_or(0)
}
fn add(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    Ok(Amount::checked(a + b)?.0)
}
fn sub(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    if a < b {
        return Err(err("MANAGED_V2_SCHEDULE_UNDERFLOW"));
    }
    Ok(Amount::checked(a - b)?.0)
}
fn mul(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    Ok(Amount::checked(a * b)?.0)
}
fn progress(start: u64, end: u64, timestamp: u64) -> Result<BigUint> {
    // Match GradualValueChange's branch order, including a zero-duration jump.
    if timestamp >= end {
        return Ok(BigUint::from(ONE));
    }
    if timestamp <= start {
        return Ok(BigUint::from(0u8));
    }
    Ok(mul(&BigUint::from(timestamp - start), &BigUint::from(ONE))? / BigUint::from(end - start))
}
fn interpolate(start: u64, end: u64, progress: &BigUint) -> Result<Amount> {
    let start = BigUint::from(start);
    let end = BigUint::from(end);
    // Floor the absolute delta before subtracting on a descending schedule.
    // A reversed interpolation changes the result by one unit.
    let value = if start > end {
        sub(
            &start,
            &(mul(&sub(&start, &end)?, progress)? / BigUint::from(ONE)),
        )?
    } else {
        add(
            &start,
            &(mul(&sub(&end, &start)?, progress)? / BigUint::from(ONE)),
        )?
    };
    Amount::checked(value)
}
impl ManagedV2Schedule {
    /// Validate exposed raw shape. History and parent getter/identity consistency
    /// require the parent; starting weights intentionally have no sum constraint.
    pub fn validate(&self) -> Result<()> {
        self.pool_state_word.to_be_bytes()?;
        if !(2..=50).contains(&self.tokens.len()) {
            return Err(err("MANAGED_V2_SCHEDULE_TOKEN_COUNT"));
        }
        let p = &self.pool_state_word;
        if field(p, 32, 32) < field(p, 0, 32) || field(p, 96, 32) < field(p, 64, 32) {
            return Err(err("MANAGED_V2_SCHEDULE_TIME_ORDER"));
        }
        for offset in [128, 190] {
            if !(MIN_FEE..=MAX_FEE).contains(&field(p, offset, 62)) {
                return Err(err("MANAGED_V2_SCHEDULE_FEE_RANGE"));
            }
        }
        let mut previous: Option<&str> = None;
        let mut end_sum = BigUint::from(0u8);
        for t in &self.tokens {
            validate_address(&t.token)?;
            if previous.is_some_and(|p| p >= t.token.as_str()) {
                return Err(err("MANAGED_V2_SCHEDULE_TOKEN_ORDER"));
            }
            previous = Some(&t.token);
            t.state_word.to_be_bytes()?;
            if t.state_word.0.bits() > 133 {
                return Err(err("MANAGED_V2_SCHEDULE_TOKEN_RESERVED"));
            }
            if field(&t.state_word, 128, 5) > 18 {
                return Err(err("MANAGED_V2_SCHEDULE_DECIMALS"));
            }
            for offset in [0, 64] {
                if !(MIN_WEIGHT..=ONE).contains(&field(&t.state_word, offset, 64)) {
                    return Err(err("MANAGED_V2_SCHEDULE_WEIGHT_RANGE"));
                }
            }
            end_sum = add(&end_sum, &BigUint::from(field(&t.state_word, 64, 64)))?;
        }
        if end_sum != BigUint::from(ONE) {
            return Err(err("MANAGED_V2_SCHEDULE_END_SUM"));
        }
        Ok(())
    }

    /// Pure execution-time projection; no normalization or canonical hash input.
    pub fn at(&self, timestamp: u64) -> Result<ManagedV2ScheduleView> {
        self.validate()?;
        timestamp
            .checked_mul(1000)
            .ok_or_else(|| err("MANAGED_V2_SCHEDULE_TIME_OVERFLOW"))?;
        let p = &self.pool_state_word;
        let weight_start = field(p, 0, 32);
        let weight_end = field(p, 32, 32);
        let fee_start = field(p, 64, 32);
        let fee_end = field(p, 96, 32);
        let weight_progress = progress(weight_start, weight_end, timestamp)?;
        let fee_progress = progress(fee_start, fee_end, timestamp)?;
        let mut weights = Vec::with_capacity(self.tokens.len());
        let mut scaling = Vec::with_capacity(self.tokens.len());
        for t in &self.tokens {
            weights.push(interpolate(
                field(&t.state_word, 0, 64),
                field(&t.state_word, 64, 64),
                &weight_progress,
            )?);
            // Repeated SafeMath-checked multiplication, including the ONE factor.
            let mut scale = BigUint::from(ONE);
            for _ in 0..field(&t.state_word, 128, 5) {
                scale = mul(&scale, &BigUint::from(10u8))?;
            }
            scaling.push(Amount::checked(scale)?);
        }
        Ok(ManagedV2ScheduleView {
            weights,
            scaling,
            swap_fee: interpolate(field(p, 128, 62), field(p, 190, 62), &fee_progress)?,
            weight_start,
            weight_end,
            fee_start,
            fee_end,
            swaps_enabled: field(p, 252, 1) != 0,
            must_allowlist_lps: field(p, 253, 1) != 0,
            recovery_mode: field(p, 254, 1) != 0,
            join_exit_enabled: field(p, 255, 1) != 0,
        })
    }
}

impl ManagedV2Schedule {
    pub fn to_proto(&self) -> Result<crate::wire::ManagedV2Schedule> {
        self.validate()?;
        Ok(crate::wire::ManagedV2Schedule {
            pool_state_word: Some(crate::wire::Amount {
                uint256_be: self.pool_state_word.to_be_bytes()?.to_vec(),
            }),
            tokens: self
                .tokens
                .iter()
                .map(|row| {
                    Ok(crate::wire::ManagedV2ScheduledToken {
                        token: row.token.clone(),
                        state_word: Some(crate::wire::Amount {
                            uint256_be: row.state_word.to_be_bytes()?.to_vec(),
                        }),
                    })
                })
                .collect::<Result<_>>()?,
        })
    }

    pub fn from_proto(w: crate::wire::ManagedV2Schedule) -> Result<Self> {
        fn word(value: Option<crate::wire::Amount>) -> Result<Amount> {
            Amount::from_be_bytes(
                &value
                    .ok_or_else(|| err("MANAGED_V2_SCHEDULE_AMOUNT_REQUIRED"))?
                    .uint256_be,
            )
        }
        let schedule = Self {
            pool_state_word: word(w.pool_state_word)?,
            tokens: w
                .tokens
                .into_iter()
                .map(|row| {
                    Ok(ManagedV2ScheduledToken {
                        token: row.token,
                        state_word: word(row.state_word)?,
                    })
                })
                .collect::<Result<_>>()?,
        };
        schedule.validate()?;
        Ok(schedule)
    }
}
