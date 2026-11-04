//! Integer port of _get_p/upkeep_oracles/_calc_moving_average/exp at the pin in
//! curve_ng.rs. Checked uint256 and explicit EVM unsafe operations stay distinct.
//! The exp polynomial is derived from the original AGPL-3.0 Curve/Snekmate code;
//! original-helpers.vy and source pin evidence retain its upstream attribution.
use super::NgOracleState;
use crate::curve_multi::{add, div, ensure, mul, sub, u, PRECISION};
use num_bigint::{BigInt, BigUint, Sign};
use num_traits::{ToPrimitive, Zero};
use xone_domain::{err, Result};

fn modulus() -> BigUint {
    BigUint::from(1u8) << 256usize
}
fn wrap_u(value: BigUint) -> BigUint {
    value % modulus()
}
fn unsafe_div(a: &BigUint, b: &BigUint) -> BigUint {
    if b.is_zero() {
        u(0)
    } else {
        a / b
    }
}
fn split(value: &BigUint) -> (BigUint, BigUint) {
    let mask = (BigUint::from(1u8) << 128usize) - BigUint::from(1u8);
    (value & mask, value >> 128usize)
}
fn pack(spot: &BigUint, ema: &BigUint) -> Result<BigUint> {
    if spot.bits() > 128 || ema.bits() > 128 {
        return Err(err("CURVE_NG_PACK128_OVERFLOW"));
    }
    Ok(spot | (ema << 128usize))
}

/// Marginal prices, preserving _get_p's D/n**n floor BEFORE the Dr loop.
/// Unlike invariant(), ANN uses unsafe_mul and may wrap for external probes.
pub fn get_p(xp: &[BigUint], amp: &BigUint, d: &BigUint) -> Result<Vec<BigUint>> {
    if !(2..=8).contains(&xp.len()) {
        return Err(err("CURVE_NG_INVALID_COIN_COUNT"));
    }
    for value in xp.iter().chain([amp, d]) {
        ensure(value)?;
    }
    let n = u(xp.len() as u64);
    let ann = wrap_u(amp * &n);
    let mut dr = d / n.pow(xp.len() as u32);
    for x in xp {
        dr = div(&mul(&dr, d)?, x)?;
    }
    let xp0_a = unsafe_div(&mul(&ann, &xp[0])?, &u(100));
    let denominator = add(&xp0_a, &dr)?;
    xp.iter()
        .skip(1)
        .map(|x| {
            let numerator = mul(
                &u(PRECISION),
                &add(&xp0_a, &unsafe_div(&mul(&dr, &xp[0])?, x))?,
            )?;
            div(&numerator, &denominator)
        })
        .collect()
}

fn moving_average(packed: &BigUint, window: u64, last: &BigUint, now: u64) -> Result<BigUint> {
    let (spot, ema) = split(packed);
    let now = u(now);
    if last >= &now {
        return Ok(ema);
    }
    let dt = &now - last;
    let exponent = unsafe_div(&wrap_u(dt * u(PRECISION)), &u(window));
    // convert(uint256,int256), then checked unary negation in original Vyper.
    if exponent.bits() > 255 {
        return Err(err("CURVE_NG_EMA_SIGNED_CONVERSION"));
    }
    let alpha = wad_exp(&-BigInt::from(exponent))?;
    // These multiplications/addition are checked despite final unsafe_div.
    Ok(unsafe_div(
        &add(
            &mul(&spot, &sub(&u(PRECISION), &alpha)?)?,
            &mul(&ema, &alpha)?,
        )?,
        &u(PRECISION),
    ))
}

pub(super) fn upkeep(
    xp: &[BigUint],
    amp: &BigUint,
    d: &BigUint,
    state: &mut NgOracleState,
) -> Result<()> {
    let (price_time, d_time) = split(&state.ma_last_time);
    let spots = get_p(xp, amp, d)?;
    for (index, spot) in spots.iter().enumerate() {
        if !spot.is_zero() {
            let ema = moving_average(
                &state.last_prices_packed[index],
                state.ma_exp_time,
                &price_time,
                state.timestamp,
            )?;
            state.last_prices_packed[index] = pack(&spot.clone().min(u(2 * PRECISION)), &ema)?;
        }
    }
    let ema_d = moving_average(
        &state.last_d_packed,
        state.d_ma_time,
        &d_time,
        state.timestamp,
    )?;
    state.last_d_packed = pack(d, &ema_d)?;
    let now = u(state.timestamp);
    state.ma_last_time = pack(&price_time.max(now.clone()), &d_time.max(now))?;
    Ok(())
}

fn z(s: &str) -> BigInt {
    s.parse().expect("original integer literal")
}
fn signed(value: BigInt) -> BigInt {
    let m = BigInt::from(modulus());
    let half = BigInt::from(BigUint::from(1u8) << 255usize);
    let v = ((value % &m) + &m) % &m;
    if v >= half {
        v - m
    } else {
        v
    }
}
fn unsigned(value: BigInt) -> BigUint {
    let m = BigInt::from(modulus());
    (((value % &m) + &m) % &m)
        .to_biguint()
        .expect("unsigned EVM word")
}
fn sadd(a: &BigInt, b: &BigInt) -> BigInt {
    signed(a + b)
}
fn ssub(a: &BigInt, b: &BigInt) -> BigInt {
    signed(a - b)
}
fn smul(a: &BigInt, b: &BigInt) -> BigInt {
    signed(a * b)
}
fn sdiv(a: &BigInt, b: &BigInt) -> BigInt {
    if b.is_zero() {
        BigInt::zero()
    } else {
        signed(a / b)
    }
}
fn shl(a: &BigInt, n: usize) -> BigInt {
    signed(a << n)
}

/// Original signed wad_exp, including arithmetic shifts, SDIV truncation toward
/// zero, unsafe signed wrapping, two's-complement cast and final uint256 MUL.
pub fn wad_exp(x: &BigInt) -> Result<BigUint> {
    let half = BigInt::from(BigUint::from(1u8) << 255usize);
    if x < &-&half || x >= &half {
        return Err(err("CURVE_NG_INT256_OVERFLOW"));
    }
    if x <= &z("-41446531673892822313") {
        return Ok(u(0));
    }
    if x >= &z("135305999368893231589") {
        return Err(err("CURVE_NG_WAD_EXP_OVERFLOW"));
    }
    let mut value = sdiv(&shl(x, 78), &BigInt::from(5u8).pow(18));
    let ln2 = z("54916777467707473351141471128");
    let k = sadd(
        &sdiv(&shl(&value, 96), &ln2),
        &(BigInt::from(1u8) << 95usize),
    ) >> 96usize;
    value = ssub(&value, &smul(&k, &ln2));
    let y = sadd(
        &(smul(&sadd(&value, &z("1346386616545796478920950773328")), &value) >> 96usize),
        &z("57155421227552351082224309758442"),
    );
    let p = sadd(
        &smul(
            &sadd(
                &(smul(
                    &ssub(&sadd(&y, &value), &z("94201549194550492254356042504812")),
                    &y,
                ) >> 96usize),
                &z("28719021644029726153956944680412240"),
            ),
            &value,
        ),
        &shl(&z("4385272521454847904659076985693276"), 96),
    );
    let mut q = sadd(
        &(smul(&ssub(&value, &z("2855989394907223263936484059900")), &value) >> 96usize),
        &z("50020603652535783019961831881945"),
    );
    q = ssub(
        &(smul(&q, &value) >> 96usize),
        &z("533845033583426703283633433725380"),
    );
    q = sadd(
        &(smul(&q, &value) >> 96usize),
        &z("3604857256930695427073651918091429"),
    );
    q = ssub(
        &(smul(&q, &value) >> 96usize),
        &z("14423608567350463180887372962807573"),
    );
    q = sadd(
        &(smul(&q, &value) >> 96usize),
        &z("26449188498355588339934803723976023"),
    );
    let r = unsigned(sdiv(&p, &q));
    let shift = ssub(&BigInt::from(195u16), &k);
    if shift.sign() == Sign::Minus {
        return Err(err("CURVE_NG_EXP_SHIFT_CONVERSION"));
    }
    let product = wrap_u(
        r * z("3822833074963236453042738258902158003155416615667")
            .to_biguint()
            .unwrap(),
    );
    Ok(match shift.to_usize() {
        Some(n) if n < 256 => product >> n,
        _ => u(0),
    })
}
