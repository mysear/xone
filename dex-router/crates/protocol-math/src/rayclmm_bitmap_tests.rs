//! Search regressions against pinned 6d7a0f82 bitmap and pool search semantics.
use super::next_array;
use xone_domain::raydium_clmm::{RaydiumCLMMBitmapExtension, RaydiumCLMMState};
use xone_domain::Pool;

fn search(
    state: &xone_domain::raydium_clmm::RaydiumCLMMState,
    index: i32,
    left: bool,
) -> xone_domain::Result<i32> {
    let array = next_array(state, index, left)?;
    Ok(state.tick_arrays[array].start_tick_index / (60 * state.tick_spacing as i32))
}

fn state(spacing: u32, proof_indices: &[i32]) -> RaydiumCLMMState {
    let pool: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/rayclmm-state.json")).unwrap();
    let mut state = pool.raydium_clmm.unwrap();
    state.tick_spacing = spacing;
    state.bitmap.fill(0);
    state.bitmap_extension = Some(RaydiumCLMMBitmapExtension {
        address: pool.pool_id,
        positive: vec![0; 112],
        negative: vec![0; 112],
    });
    let template = state.tick_arrays[0].clone();
    state.tick_arrays = proof_indices
        .iter()
        .map(|index| {
            let mut array = template.clone();
            array.start_tick_index = index * 60 * spacing as i32;
            array
        })
        .collect();
    state
}

fn base(state: &mut RaydiumCLMMState, index: i32) {
    let bit = (index + 512) as usize;
    state.bitmap[bit / 64] |= 1 << (bit % 64);
}
fn error(state: &RaydiumCLMMState, index: i32, left: bool, expected: &str) {
    assert_eq!(
        search(state, index, left).unwrap_err().0,
        expected,
        "search from {index}, left={left}"
    );
}

#[test]
fn positive_base_extension_boundary_in_both_directions() {
    let mut state = state(1, &[511, 512]);
    base(&mut state, 511);
    state.bitmap_extension.as_mut().unwrap().positive[0] = 1;
    assert_eq!(search(&state, 511, false).unwrap(), 512);
    assert_eq!(search(&state, 512, true).unwrap(), 511);
}

#[test]
fn negative_base_extension_boundary_in_both_directions() {
    let mut state = state(1, &[-513, -512]);
    base(&mut state, -512);
    // First negative group is [-1024, -513], increasing index = increasing bit.
    state.bitmap_extension.as_mut().unwrap().negative[7] = 1 << 63;
    assert_eq!(search(&state, -512, true).unwrap(), -513);
    assert_eq!(search(&state, -513, false).unwrap(), -512);
}

#[test]
fn negative_whole_group_boundary_in_both_directions() {
    let mut state = state(1, &[-1025, -1024]);
    let extension = state.bitmap_extension.as_mut().unwrap();
    extension.negative[0] = 1;
    extension.negative[15] = 1 << 63;
    assert_eq!(search(&state, -1024, true).unwrap(), -1025);
    assert_eq!(search(&state, -1025, false).unwrap(), -1024);
}

#[test]
fn positive_whole_group_boundary_in_both_directions() {
    let mut state = state(1, &[1023, 1024]);
    let extension = state.bitmap_extension.as_mut().unwrap();
    extension.positive[7] = 1 << 63;
    extension.positive[8] = 1;
    assert_eq!(search(&state, 1023, false).unwrap(), 1024);
    assert_eq!(search(&state, 1024, true).unwrap(), 1023);
}

#[test]
fn first_array_search_skips_empty_extension_groups() {
    let mut state = state(1, &[-2049, 2048]);
    let extension = state.bitmap_extension.as_mut().unwrap();
    extension.positive[24] = 1;
    extension.negative[31] = 1 << 63;
    assert_eq!(search(&state, 510, false).unwrap(), 2048);
    assert_eq!(search(&state, -511, true).unwrap(), -2049);
}

#[test]
fn search_skips_empty_base_and_extension_groups_toward_zero() {
    let mut state = state(1, &[-513, 512]);
    let extension = state.bitmap_extension.as_mut().unwrap();
    extension.positive[0] = 1;
    extension.negative[7] = 1 << 63;
    assert_eq!(search(&state, -1537, false).unwrap(), -513);
    assert_eq!(search(&state, 1536, true).unwrap(), 512);
    assert_eq!(search(&state, -513, false).unwrap(), 512);
    assert_eq!(search(&state, 512, true).unwrap(), -513);
}

#[test]
fn nearest_initialized_array_requires_proof_even_if_a_later_array_is_proven() {
    let mut state = state(1, &[-1026, 1025]);
    let extension = state.bitmap_extension.as_mut().unwrap();
    extension.positive[8] = 3;
    extension.negative[15] = (1 << 63) | (1 << 62);
    error(&state, 511, false, "RAYCLMM_ARRAY_UNPROVEN");
    error(&state, -512, true, "RAYCLMM_ARRAY_UNPROVEN");
    // The search is read-only, including the unavailable array's bitmap bit.
    assert_eq!(state.bitmap_extension.as_ref().unwrap().positive[8], 3);
}

#[test]
fn high_group_set_bits_outside_proof_are_unproven() {
    let mut state = state(1, &[-513, 512]);
    let extension = state.bitmap_extension.as_mut().unwrap();
    // ±7000 fit the source tick domain, in high extension groups.
    extension.positive[101] = 1 << 24;
    extension.negative[98] = 1 << 40;
    error(&state, 511, false, "RAYCLMM_ARRAY_UNPROVEN");
    error(&state, -512, true, "RAYCLMM_ARRAY_UNPROVEN");
}

#[test]
fn tick_domain_edge_arrays_are_selected_including_negative_floor_division() {
    let mut state = state(1, &[-7394, 7393]);
    let extension = state.bitmap_extension.as_mut().unwrap();
    extension.positive[107] = 1 << 33;
    extension.negative[108] = 1 << 30;
    assert_eq!(search(&state, 511, false).unwrap(), 7393);
    assert_eq!(search(&state, -512, true).unwrap(), -7394);
}

#[test]
fn exhausted_extension_terminates_at_the_source_tick_domain() {
    for spacing in [1, 2, 3, 14, 15, 16, 128, 1000] {
        let mut state = state(spacing, &[]);
        // Full account bits beyond the source tick domain are retained, but
        // they cannot turn an exhausted source search into unproven liquidity.
        state.bitmap_extension.as_mut().unwrap().positive[111] = 1 << 63;
        state.bitmap_extension.as_mut().unwrap().negative[104] = 1;
        let span = 60 * spacing as i32;
        let min = (-443636i32).div_euclid(span);
        let max = 443636i32.div_euclid(span);
        error(&state, min, true, "RAYCLMM_INSUFFICIENT_LIQUIDITY");
        error(&state, max, false, "RAYCLMM_INSUFFICIENT_LIQUIDITY");
        error(&state, 0, true, "RAYCLMM_INSUFFICIENT_LIQUIDITY");
        error(&state, 0, false, "RAYCLMM_INSUFFICIENT_LIQUIDITY");
    }
}

#[test]
fn legacy_profiles_preserve_missing_extension_and_exhaustion_errors() {
    for spacing in [1, 2, 14, 15, 16, 128] {
        let mut state = state(spacing, &[]);
        state.bitmap_extension = None;
        let expected = if spacing < 15 {
            "RAYCLMM_BITMAP_EXTENSION_REQUIRED"
        } else {
            "RAYCLMM_INSUFFICIENT_LIQUIDITY"
        };
        error(&state, 0, true, expected);
        error(&state, 0, false, expected);
    }
}
