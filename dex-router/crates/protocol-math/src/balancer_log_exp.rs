//! Integer port of Balancer V2 LogExpMath.pow and FixedPoint.powUp at
//! e281ff40583c39bf1d46c322334b2b13936e0e33. GPL-3.0-or-later.
//! Signed division/remainder truncate toward zero, including the split ln36
//! multiplication. This module never uses floating point or a host pow/log.
use num_bigint::{BigInt, BigUint};
use num_traits::{One, Zero};
use std::sync::LazyLock;
use xone_domain::{err, Result};

static X: LazyLock<[BigInt; 12]> = LazyLock::new(|| {
    [
        "128000000000000000000",
        "64000000000000000000",
        "3200000000000000000000",
        "1600000000000000000000",
        "800000000000000000000",
        "400000000000000000000",
        "200000000000000000000",
        "100000000000000000000",
        "50000000000000000000",
        "25000000000000000000",
        "12500000000000000000",
        "6250000000000000000",
    ]
    .map(|s| s.parse().expect("fixed LogExpMath constant"))
});
static A: LazyLock<[BigInt; 12]> = LazyLock::new(|| {
    [
        "38877084059945950922200000000000000000000000000000000000",
        "6235149080811616882910000000",
        "7896296018268069516100000000000000",
        "888611052050787263676000000",
        "298095798704172827474000",
        "5459815003314423907810",
        "738905609893065022723",
        "271828182845904523536",
        "164872127070012814685",
        "128402541668774148407",
        "113314845306682631683",
        "106449445891785942956",
    ]
    .map(|s| s.parse().expect("fixed LogExpMath constant"))
});
static SIGNED_LIMIT: LazyLock<BigInt> = LazyLock::new(|| BigInt::one() << 255usize);
static SIGNED_MIN: LazyLock<BigInt> = LazyLock::new(|| -&*SIGNED_LIMIT);
fn ten(n: u32) -> BigInt {
    BigInt::from(10u8).pow(n)
}
fn signed(v: BigInt) -> Result<BigInt> {
    if v < *SIGNED_MIN || v >= *SIGNED_LIMIT {
        Err(err("WEIGHTED_SIGNED_INTERMEDIATE_OVERFLOW"))
    } else {
        Ok(v)
    }
}
fn smul(a: &BigInt, b: &BigInt) -> Result<BigInt> {
    signed(a * b)
}
fn sadd(a: &BigInt, b: &BigInt) -> Result<BigInt> {
    signed(a + b)
}
fn uint(v: BigUint) -> Result<BigUint> {
    if v.bits() > 256 {
        Err(err("WEIGHTED_INTERMEDIATE_OVERFLOW"))
    } else {
        Ok(v)
    }
}

/// Original unsigned LogExpMath.pow. Zero-exponent and zero-base cases precede
/// the signed-base/exponent checks, exactly as in the pinned Solidity source.
pub(crate) fn pow_raw(x: &BigUint, y: &BigUint) -> Result<BigUint> {
    if x.bits() > 256 || y.bits() > 256 {
        return Err(err("WEIGHTED_INTERMEDIATE_OVERFLOW"));
    }
    let one = ten(18);
    if y.is_zero() {
        return Ok(BigUint::from(10u8).pow(18));
    }
    if x.is_zero() {
        return Ok(BigUint::zero());
    }
    if x.bits() > 255 {
        return Err(err("WEIGHTED_LOGEXP_X_BOUND"));
    }
    let mild = (BigUint::one() << 254usize) / BigUint::from(10u8).pow(20);
    if y >= &mild {
        return Err(err("WEIGHTED_LOGEXP_Y_BOUND"));
    }
    let x = BigInt::from(x.clone());
    let y = BigInt::from(y.clone());
    let lower = &one - ten(17);
    let upper = &one + ten(17);
    let product = if x > lower && x < upper {
        let log = ln36(&x)?;
        sadd(
            &smul(&(&log / &one), &y)?,
            &(smul(&(&log % &one), &y)? / &one),
        )? / &one
    } else {
        smul(&ln(x)?, &y)? / &one
    };
    let result = exp(product)?;
    result
        .to_biguint()
        .ok_or_else(|| err("WEIGHTED_LOGEXP_NEGATIVE_RESULT"))
}
/// Original FixedPoint.powUp including the exact ONE/TWO/FOUR fast paths and
/// ceil(raw*1e4/ONE)+1 error allowance for every other exponent.
pub(crate) fn pow_up(x: &BigUint, y: &BigUint) -> Result<BigUint> {
    if x.bits() > 256 || y.bits() > 256 {
        return Err(err("WEIGHTED_INTERMEDIATE_OVERFLOW"));
    }
    let one = BigUint::from(10u8).pow(18);
    let mul_up = |a: &BigUint, b: &BigUint| -> Result<BigUint> {
        let p = uint(a * b)?;
        Ok(if p.is_zero() {
            p
        } else {
            (p - BigUint::one()) / &one + BigUint::one()
        })
    };
    if y == &one {
        return Ok(x.clone());
    }
    if y == &(&one * 2u8) {
        return mul_up(x, x);
    }
    if y == &(&one * 4u8) {
        let square = mul_up(x, x)?;
        return mul_up(&square, &square);
    }
    let raw = pow_raw(x, y)?;
    let allowance = uint(mul_up(&raw, &BigUint::from(10_000u32))? + BigUint::one())?;
    uint(raw + allowance)
}
/// Original FixedPoint.powDown, including optimized exponents and the
/// relative-error subtraction used by WeightedMath's partial fee invariant.
pub(crate) fn pow_down(x: &BigUint, y: &BigUint) -> Result<BigUint> {
    uint(x.clone())?;
    uint(y.clone())?;
    let one = BigUint::from(10u8).pow(18);
    let mul_down = |a: &BigUint, b: &BigUint| -> Result<BigUint> { Ok(uint(a * b)? / &one) };
    if y == &one {
        return Ok(x.clone());
    }
    if y == &(&one * 2u8) {
        return mul_down(x, x);
    }
    if y == &(&one * 4u8) {
        let square = mul_down(x, x)?;
        return mul_down(&square, &square);
    }
    let raw = pow_raw(x, y)?;
    let product = uint(&raw * BigUint::from(10_000u32))?;
    let rounded = if product.is_zero() {
        product
    } else {
        (product - BigUint::one()) / &one + BigUint::one()
    };
    let allowance = uint(rounded + BigUint::one())?;
    Ok(if raw < allowance {
        BigUint::zero()
    } else {
        raw - allowance
    })
}
fn exp(mut x: BigInt) -> Result<BigInt> {
    let one = ten(18);
    let work = ten(20);
    if x < -(&one * 41u8) || x > &one * 130u8 {
        return Err(err("WEIGHTED_LOGEXP_PRODUCT_BOUND"));
    }
    if x < BigInt::zero() {
        return Ok(smul(&one, &one)? / exp(-x)?);
    }
    let first = if x >= X[0] {
        x -= &X[0];
        A[0].clone()
    } else if x >= X[1] {
        x -= &X[1];
        A[1].clone()
    } else {
        BigInt::one()
    };
    x = smul(&x, &BigInt::from(100u8))?;
    let mut product = work.clone();
    // exp intentionally stops at x9; x10/x11 belong only to ln.
    for i in 2..=9 {
        if x >= X[i] {
            x -= &X[i];
            product = smul(&product, &A[i])? / &work;
        }
    }
    let mut term = x.clone();
    let mut series = sadd(&work, &term)?;
    for denominator in 2u8..=12 {
        term = smul(&term, &x)? / &work / denominator;
        series = sadd(&series, &term)?;
    }
    Ok(smul(&(smul(&product, &series)? / &work), &first)? / 100u8)
}
fn ln(mut a: BigInt) -> Result<BigInt> {
    let one = ten(18);
    let work = ten(20);
    if a <= BigInt::zero() {
        return Err(err("WEIGHTED_LOGEXP_LN_BOUND"));
    }
    if a < one {
        return Ok(-ln(smul(&one, &one)? / a)?);
    }
    let mut sum = BigInt::zero();
    for i in 0..=1 {
        if a >= smul(&A[i], &one)? {
            a /= &A[i];
            sum = sadd(&sum, &X[i])?;
        }
    }
    sum = smul(&sum, &BigInt::from(100u8))?;
    a = smul(&a, &BigInt::from(100u8))?;
    for i in 2..=11 {
        if a >= A[i] {
            a = smul(&a, &work)? / &A[i];
            sum = sadd(&sum, &X[i])?;
        }
    }
    let z = smul(&(&a - &work), &work)? / sadd(&a, &work)?;
    let z_squared = smul(&z, &z)? / &work;
    let mut num = z.clone();
    let mut series = num.clone();
    for denominator in [3u8, 5, 7, 9, 11] {
        num = smul(&num, &z_squared)? / &work;
        series = sadd(&series, &(&num / denominator))?;
    }
    Ok(sadd(&sum, &smul(&series, &BigInt::from(2u8))?)? / 100u8)
}
fn ln36(x: &BigInt) -> Result<BigInt> {
    let work = ten(36);
    let x = smul(x, &ten(18))?;
    let z = smul(&(&x - &work), &work)? / sadd(&x, &work)?;
    let z_squared = smul(&z, &z)? / &work;
    let mut num = z.clone();
    let mut series = num.clone();
    for denominator in [3u8, 5, 7, 9, 11, 13, 15] {
        num = smul(&num, &z_squared)? / &work;
        series = sadd(&series, &(&num / denominator))?;
    }
    smul(&series, &BigInt::from(2u8))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_solidity_logexp_and_pow_up_match_branch_and_domain_vectors() {
        let data: serde_json::Value = serde_json::from_str(include_str!(
            "../../../test/contracts/weighted-static-powers.json"
        ))
        .unwrap();
        let cases = data["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 240);
        for (i, c) in cases.iter().enumerate() {
            let x: BigUint = c["x"].as_str().unwrap().parse().unwrap();
            let y: BigUint = c["y"].as_str().unwrap().parse().unwrap();
            for (name, result) in [("raw", pow_raw(&x, &y)), ("up", pow_up(&x, &y))] {
                if c[name]["reverted"] == true {
                    assert!(result.is_err(), "{i} {name} should reject");
                } else {
                    assert_eq!(
                        result
                            .unwrap_or_else(|e| panic!("case {i} {name}: {e}"))
                            .to_string(),
                        c[name]["value"].as_str().unwrap(),
                        "case {i} {name}"
                    );
                }
            }
        }
    }
}
