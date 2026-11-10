//! Original 20230411 circuit storage and six unscaled getter observations.
use crate::{err, protocols::validate_address, wire, Amount, Result};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
pub const PROTOCOL: &str = "balancer-v2-managed-breaker";
pub const ADAPTER_VERSION: &str = "balancer-v2/20230411-managed-breaker-1";
const ONE: u64 = 1_000_000_000_000_000_000;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagedV2CircuitToken {
    pub token: String,
    pub state_word: Amount,
    pub reference_bpt_price: Amount,
    pub reference_weight: Amount,
    pub lower_percentage: Amount,
    pub upper_percentage: Amount,
    pub lower_bpt_price_bound: Amount,
    pub upper_bpt_price_bound: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagedV2CircuitState {
    pub tokens: Vec<ManagedV2CircuitToken>,
}
fn field(word: &BigUint, offset: usize, width: usize) -> BigUint {
    (word >> offset) & ((BigUint::from(1u8) << width) - BigUint::from(1u8))
}
fn mul_div(a: &BigUint, b: &BigUint, denominator: &BigUint, ceil: bool) -> Result<BigUint> {
    let product = a * b;
    if product.bits() > 256 {
        return Err(err("MANAGED_V2_CIRCUIT_MUL_OVERFLOW"));
    }
    let mut quotient = &product / denominator;
    if ceil && &product % denominator != BigUint::from(0u8) {
        quotient += BigUint::from(1u8);
    }
    Ok(quotient)
}
impl ManagedV2CircuitToken {
    fn validate(&self) -> Result<()> {
        validate_address(&self.token)?;
        for amount in [
            &self.state_word,
            &self.reference_bpt_price,
            &self.reference_weight,
            &self.lower_percentage,
            &self.upper_percentage,
            &self.lower_bpt_price_bound,
            &self.upper_bpt_price_bound,
        ] {
            if amount.0.bits() > 256 {
                return Err(err("AMOUNT_OVERFLOW"));
            }
        }
        let zero = BigUint::from(0u8);
        let word = &self.state_word.0;
        let lower = field(word, 0, 16);
        let upper = field(word, 16, 16);
        let weight = field(word, 32, 64);
        let price = field(word, 96, 96);
        // Compare compressed endpoints: decompress(compress(1e17)) is below 1e17.
        if lower != zero
            && (lower < BigUint::from(100_000_000_000_000_000u64) >> 48
                || lower > BigUint::from(ONE) >> 48)
            || upper != zero
                && (upper < lower || upper > (BigUint::from(ONE) * BigUint::from(10u8)) >> 48)
        {
            return Err(err("MANAGED_V2_CIRCUIT_PERCENTAGE_RANGE"));
        }
        if *word != zero
            && (weight < BigUint::from(10_000_000_000_000_000u64) || weight > BigUint::from(ONE))
        {
            return Err(err("MANAGED_V2_CIRCUIT_REFERENCE_WEIGHT"));
        }
        if self.reference_weight.0 != weight
            || self.lower_percentage.0 != lower << 48
            || self.upper_percentage.0 != upper << 48
        {
            return Err(err("MANAGED_V2_CIRCUIT_RAW_GETTER"));
        }
        if price == zero && self.reference_bpt_price.0 != zero
            || (self.lower_percentage.0 == zero || price == zero)
                && self.lower_bpt_price_bound.0 != zero
            || (self.upper_percentage.0 == zero || price == zero)
                && self.upper_bpt_price_bound.0 != zero
        {
            return Err(err("MANAGED_V2_CIRCUIT_DISABLED_GETTER"));
        }
        Ok(())
    }
}
impl ManagedV2CircuitState {
    pub fn validate(&self) -> Result<()> {
        if !(2..=50).contains(&self.tokens.len()) {
            return Err(err("MANAGED_V2_CIRCUIT_TOKEN_COUNT"));
        }
        let mut previous: Option<&str> = None;
        for token in &self.tokens {
            if previous.is_some_and(|p| p >= token.token.as_str()) {
                return Err(err("MANAGED_V2_CIRCUIT_TOKEN_ORDER"));
            }
            token.validate()?;
            previous = Some(&token.token);
        }
        Ok(())
    }
    pub(crate) fn validate_pool(
        &self,
        assets: &[crate::multi_asset::MultiAsset],
        weights: &[Amount],
    ) -> Result<()> {
        self.validate()?;
        if self.tokens.len() != assets.len() || weights.len() != assets.len() {
            return Err(err("MANAGED_V2_CIRCUIT_TOKEN_COUNT"));
        }
        let one = BigUint::from(ONE);
        let zero = BigUint::from(0u8);
        for ((token, asset), weight) in self.tokens.iter().zip(assets).zip(weights) {
            if token.token != asset.token {
                return Err(err("MANAGED_V2_CIRCUIT_TOKEN_IDENTITY"));
            }
            let price = field(&token.state_word.0, 96, 96);
            if mul_div(&price, &asset.scaling.0, &one, false)? != token.reference_bpt_price.0 {
                return Err(err("MANAGED_V2_CIRCUIT_PRICE_GETTER"));
            }
            // Dynamic bounds require the original fixed-point pow in pricing.
            // Only required/range/raw/disabled-side validation is done here.
            if *weight != token.reference_weight {
                continue;
            }
            for (percentage, offset, getter, ceil) in [
                (
                    &token.lower_percentage,
                    192,
                    &token.lower_bpt_price_bound,
                    true,
                ),
                (
                    &token.upper_percentage,
                    224,
                    &token.upper_bpt_price_bound,
                    false,
                ),
            ] {
                if percentage.0 == zero || price == zero {
                    continue;
                }
                let ratio = mul_div(
                    &field(&token.state_word.0, offset, 32),
                    &(&one * BigUint::from(10u8)),
                    &BigUint::from(u32::MAX),
                    false,
                )?;
                let boundary = mul_div(&price, &ratio, &one, ceil)?;
                if mul_div(&boundary, &asset.scaling.0, &one, false)? != getter.0 {
                    return Err(err("MANAGED_V2_CIRCUIT_BOUND_GETTER"));
                }
            }
        }
        Ok(())
    }
    pub fn to_proto(&self) -> Result<wire::ManagedV2CircuitState> {
        self.validate()?;
        fn amount(a: &Amount) -> Result<Option<wire::Amount>> {
            Ok(Some(wire::Amount {
                uint256_be: a.to_be_bytes()?.to_vec(),
            }))
        }
        Ok(wire::ManagedV2CircuitState {
            tokens: self
                .tokens
                .iter()
                .map(|t| {
                    Ok(wire::ManagedV2CircuitToken {
                        token: t.token.clone(),
                        state_word: amount(&t.state_word)?,
                        reference_bpt_price: amount(&t.reference_bpt_price)?,
                        reference_weight: amount(&t.reference_weight)?,
                        lower_percentage: amount(&t.lower_percentage)?,
                        upper_percentage: amount(&t.upper_percentage)?,
                        lower_bpt_price_bound: amount(&t.lower_bpt_price_bound)?,
                        upper_bpt_price_bound: amount(&t.upper_bpt_price_bound)?,
                    })
                })
                .collect::<Result<_>>()?,
        })
    }
    pub fn from_proto(w: wire::ManagedV2CircuitState) -> Result<Self> {
        fn amount(a: Option<wire::Amount>) -> Result<Amount> {
            Amount::from_be_bytes(
                &a.ok_or_else(|| err("MANAGED_V2_CIRCUIT_AMOUNT_REQUIRED"))?
                    .uint256_be,
            )
        }
        let s = Self {
            tokens: w
                .tokens
                .into_iter()
                .map(|t| {
                    Ok(ManagedV2CircuitToken {
                        token: t.token,
                        state_word: amount(t.state_word)?,
                        reference_bpt_price: amount(t.reference_bpt_price)?,
                        reference_weight: amount(t.reference_weight)?,
                        lower_percentage: amount(t.lower_percentage)?,
                        upper_percentage: amount(t.upper_percentage)?,
                        lower_bpt_price_bound: amount(t.lower_bpt_price_bound)?,
                        upper_bpt_price_bound: amount(t.upper_bpt_price_bound)?,
                    })
                })
                .collect::<Result<_>>()?,
        };
        s.validate()?;
        Ok(s)
    }
}
