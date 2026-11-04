//! Bounded regular GIVEN_IN swaps for the original Balancer ManagedPool at
//! balancer/balancer-v2-monorepo e281ff40583c39bf1d46c322334b2b13936e0e33.
//! Reproduces ManagedPool._onSwapGivenIn/_payProtocolAndManagementFees and
//! WeightedMath._calculateInvariant/_calcDueProtocolSwapFeeBptAmount. Fee
//! invariants use the selected internal scaled pair: reconstructed gross input
//! and output before downscaling. Caller establishes pool/runtime identity,
//! current weights/fees, unpaused state and enabled swaps. Regular swaps do not
//! collect AUM fees; AUM collection and joins/exits are outside this API.
use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::{err, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedFeeState {
    pub total_supply: BigUint,
    pub protocol_swap_fee: BigUint,
    pub management_swap_fee: BigUint,
    pub owner_balance: BigUint,
    pub protocol_collector_balance: BigUint,
    /// Both balance fields describe the same ERC20 account when true.
    pub same_recipient: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedFeeQuote {
    pub amount_out: BigUint,
    pub next_balances: Vec<BigUint>,
    pub next_state: ManagedFeeState,
    pub protocol_bpt: BigUint,
    pub management_bpt: BigUint,
}
const ONE: u64 = 1_000_000_000_000_000_000;
fn checked(n: BigUint) -> Result<BigUint> {
    if n.bits() > 256 {
        Err(err("BALANCER_MANAGED_UINT256"))
    } else {
        Ok(n)
    }
}
fn add(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    checked(a + b)
}
fn mul(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    checked(a * b)
}
fn ceil(n: BigUint, d: &BigUint) -> Result<BigUint> {
    if d.is_zero() {
        return Err(err("BALANCER_MANAGED_ZERO_DIVISION"));
    }
    Ok(if n.is_zero() {
        n
    } else {
        (n - BigUint::one()) / d + BigUint::one()
    })
}
fn md(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    Ok(mul(a, b)? / ONE)
}
fn mu(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    ceil(mul(a, b)?, &ONE.into())
}
fn dd(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    if b.is_zero() {
        return Err(err("BALANCER_MANAGED_ZERO_DIVISION"));
    }
    Ok(mul(a, &ONE.into())? / b)
}
fn du(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    ceil(mul(a, &ONE.into())?, b)
}
fn invariant(b: [&BigUint; 2], w: [&BigUint; 2]) -> Result<BigUint> {
    let mut inv = BigUint::from(ONE);
    for k in 0..2 {
        inv = md(&inv, &crate::balancer_log_exp::pow_down(b[k], w[k])?)?;
    }
    if inv.is_zero() {
        return Err(err("BALANCER_MANAGED_ZERO_INVARIANT"));
    }
    Ok(inv)
}

/// 2..=38 tokens, standard decimal scales ONE*10^(18-decimals), decimals 0..18.
/// The profile admits raw swap fees 1e12..=8e17, protocol shares 0..=5e17,
/// and management shares 0..=ONE. Native successful zero outputs are retained.
/// Original compressed Managed weights may have rounding drift in their sum.
/// No state is mutated on error, including a failure after either BPT mint.
#[allow(clippy::too_many_arguments)]
pub fn quote_exact_in(
    balances: &[BigUint],
    weights: &[BigUint],
    scales: &[BigUint],
    fee: &BigUint,
    input: &BigUint,
    i: usize,
    j: usize,
    state: &ManagedFeeState,
) -> Result<ManagedFeeQuote> {
    let one = BigUint::from(ONE);
    for v in [
        &state.total_supply,
        &state.protocol_swap_fee,
        &state.management_swap_fee,
        &state.owner_balance,
        &state.protocol_collector_balance,
    ] {
        checked(v.clone())?;
    }
    if state.total_supply.is_zero()
        || state.protocol_swap_fee > BigUint::from(500_000_000_000_000_000u64)
        || state.management_swap_fee > one
        || state.owner_balance > state.total_supply
        || state.protocol_collector_balance > state.total_supply
        || (!state.same_recipient
            && &state.owner_balance + &state.protocol_collector_balance > state.total_supply)
        || (state.same_recipient && state.owner_balance != state.protocol_collector_balance)
    {
        return Err(err("BALANCER_MANAGED_FEE_STATE"));
    }
    if fee < &BigUint::from(1_000_000_000_000u64)
        || fee > &BigUint::from(800_000_000_000_000_000u64)
    {
        return Err(err("BALANCER_MANAGED_SWAP_FEE"));
    }
    let count = balances.len();
    if !(2..=38).contains(&count) || weights.len() != count || scales.len() != count {
        return Err(err("BALANCER_MANAGED_LENGTH"));
    }
    if i >= count || j >= count || i == j {
        return Err(err("BALANCER_MANAGED_INDEX"));
    }
    checked(input.clone())?;
    for k in 0..count {
        checked(weights[k].clone())?;
        if weights[k].is_zero() || weights[k] > one {
            return Err(err("BALANCER_MANAGED_WEIGHT"));
        }
        if !(0..=18).any(|d| scales[k] == &one * BigUint::from(10u8).pow(d)) {
            return Err(err("BALANCER_MANAGED_SCALE"));
        }
        if balances[k].bits() > 112 {
            return Err(err("BALANCER_MANAGED_UINT112"));
        }
        if balances[k].is_zero() {
            return Err(err("BALANCER_MANAGED_ZERO_BALANCE"));
        }
    }
    let bi = md(&balances[i], &scales[i])?;
    let bo = md(&balances[j], &scales[j])?;
    let net = md(&(input - mu(input, fee)?), &scales[i])?;
    if net > md(&bi, &BigUint::from(300_000_000_000_000_000u64))? {
        return Err(err("BALANCER_MANAGED_MAX_IN_RATIO"));
    }
    let base = du(&bi, &add(&bi, &net)?)?;
    let exponent = dd(&weights[i], &weights[j])?;
    let power = crate::balancer_log_exp::pow_up(&base, &exponent)?;
    let complement = if power >= one {
        BigUint::zero()
    } else {
        &one - power
    };
    let out = md(&bo, &complement)?;
    let amount_out = dd(&out, &scales[j])?;
    if amount_out > balances[j] {
        return Err(err("BALANCER_MANAGED_SUB_OVERFLOW"));
    }
    // The wrapper updates raw Vault cash after the callback. Any error rolls
    // back the callback's fee mints as well as cash, so return no partial state.
    let mut next_balances = balances.to_vec();
    next_balances[i] = add(&balances[i], input)?;
    if next_balances[i].bits() > 112 {
        return Err(err("BALANCER_MANAGED_UINT112"));
    }
    next_balances[j] = &balances[j] - &amount_out;
    let mut next_state = state.clone();
    let (mut protocol_bpt, mut management_bpt) = (BigUint::zero(), BigUint::zero());
    if !state.protocol_swap_fee.is_zero() || !state.management_swap_fee.is_zero() {
        let gross = du(&net, &(&one - fee))?;
        let post_in = add(&bi, &gross)?;
        if out > bo {
            return Err(err("BALANCER_MANAGED_SUB_OVERFLOW"));
        }
        let post_out = &bo - out;
        let pre = invariant([&bi, &bo], [&weights[i], &weights[j]])?;
        let post = invariant([&post_in, &post_out], [&weights[i], &weights[j]])?;
        let total_fee = add(
            &state.protocol_swap_fee,
            &md(
                &(&one - &state.protocol_swap_fee),
                &state.management_swap_fee,
            )?,
        )?;
        let growth = dd(&post, &pre)?;
        let total_bpt = if growth <= one {
            BigUint::zero()
        } else {
            let k = dd(&md(&total_fee, &(&growth - &one))?, &growth)?;
            let denominator = if k >= one { BigUint::zero() } else { &one - &k };
            if denominator.is_zero() {
                BigUint::zero()
            } else {
                dd(&md(&state.total_supply, &k)?, &denominator)?
            }
        };
        protocol_bpt = mu(&total_bpt, &du(&state.protocol_swap_fee, &total_fee)?)?;
        if !state.management_swap_fee.is_zero() {
            if protocol_bpt > total_bpt {
                return Err(err("BALANCER_MANAGED_SUB_OVERFLOW"));
            }
            management_bpt = &total_bpt - &protocol_bpt;
        }
        // Original mint order is protocol then management. Preserve every
        // checked addition rather than checking only the final combined value.
        next_state.total_supply = add(&next_state.total_supply, &protocol_bpt)?;
        next_state.protocol_collector_balance =
            add(&next_state.protocol_collector_balance, &protocol_bpt)?;
        if state.same_recipient {
            next_state.owner_balance = next_state.protocol_collector_balance.clone();
        }
        next_state.total_supply = add(&next_state.total_supply, &management_bpt)?;
        next_state.owner_balance = add(&next_state.owner_balance, &management_bpt)?;
        if state.same_recipient {
            next_state.protocol_collector_balance = next_state.owner_balance.clone();
        }
    }
    Ok(ManagedFeeQuote {
        amount_out,
        next_balances,
        next_state,
        protocol_bpt,
        management_bpt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_selected_scaled_partial_invariants_match_exactly() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test/contracts/balancer-managed-fees.json");
        let data: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let number = |v: &serde_json::Value| v.as_str().unwrap().parse::<BigUint>().unwrap();
        let mut compared = 0;
        for c in data["cases"].as_array().unwrap() {
            if c["expected"]["reverted"] == true {
                continue;
            }
            let i = c["index_in"].as_u64().unwrap() as usize;
            let j = c["index_out"].as_u64().unwrap() as usize;
            let wi = number(&c["weights"][i]);
            let wo = number(&c["weights"][j]);
            let v = &c["internal"];
            let pre = [number(&v["pre_balances"][0]), number(&v["pre_balances"][1])];
            let post = [
                number(&v["post_balances"][0]),
                number(&v["post_balances"][1]),
            ];
            assert_eq!(
                invariant([&pre[0], &pre[1]], [&wi, &wo]).unwrap(),
                number(&v["pre_invariant"])
            );
            assert_eq!(
                invariant([&post[0], &post[1]], [&wi, &wo]).unwrap(),
                number(&v["post_invariant"])
            );
            let input = number(&c["input"]);
            let fee = number(&c["fee"]);
            let si = number(&c["scales"][i]);
            let net = md(&(&input - mu(&input, &fee).unwrap()), &si).unwrap();
            assert_eq!(net, number(&v["net_scaled"]));
            assert_eq!(
                du(&net, &(BigUint::from(ONE) - fee)).unwrap(),
                number(&v["reconstructed_gross_scaled"])
            );
            compared += 1;
        }
        assert!(compared >= 200);
    }
}
