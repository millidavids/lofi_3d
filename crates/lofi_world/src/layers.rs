//! Four-way weight arithmetic: blending one material into a vertex.
//!
//! Used by [`crate::VertexPaint`], and to read pre-v3 maps, which kept four
//! map-wide layers with weights per vertex.

pub const LAYER_COUNT: usize = 4;

/// How much of each of four materials a vertex shows. Sums to 255.
pub type LayerWeights = [u8; LAYER_COUNT];

const FIRST_ONLY: LayerWeights = [255, 0, 0, 0];

/// Moves `weights` toward all-`layer` by `amount` (0..1).
///
/// `dither` (0..1) decides which way the fractional step rounds. Pass a fresh
/// random value per vertex per frame so a light brush lands on average
/// instead of rounding to nothing near full coverage. 0.5 rounds to nearest.
#[must_use]
pub fn paint_toward(weights: LayerWeights, layer: usize, amount: f32, dither: f32) -> LayerWeights {
    let amount = amount.clamp(0.0, 1.0);
    let old = u32::from(weights[layer]);
    if amount <= 0.0 || old >= 255 {
        return weights;
    }

    let stepped = (255 - old) as f32 * amount + dither.clamp(0.0, 0.999);
    let target = (old + stepped as u32).min(255);
    if target == old {
        return weights;
    }

    // Others shrink in proportion. `others_old` is nonzero because old < 255.
    let others_old = 255 - old;
    let others_new = 255 - target;
    let mut out = [0u8; LAYER_COUNT];
    out[layer] = target as u8;
    let mut assigned = 0;
    let mut largest: Option<usize> = None;
    for i in (0..LAYER_COUNT).filter(|&i| i != layer) {
        let scaled = u32::from(weights[i]) * others_new / others_old;
        out[i] = scaled as u8;
        assigned += scaled;
        if largest.is_none_or(|j| weights[i] > weights[j]) {
            largest = Some(i);
        }
    }
    if let Some(i) = largest {
        out[i] += (others_new - assigned) as u8;
    }
    out
}

/// Rescales weights to sum to 255. All-zero goes wholly to the first.
#[must_use]
pub fn normalize_weights(weights: LayerWeights) -> LayerWeights {
    let sum: u32 = weights.iter().map(|&w| u32::from(w)).sum();
    if sum == 255 {
        return weights;
    }
    if sum == 0 {
        return FIRST_ONLY;
    }
    let mut out = weights.map(|w| (u32::from(w) * 255 / sum) as u8);
    let assigned: u32 = out.iter().map(|&w| u32::from(w)).sum();
    let largest = (0..LAYER_COUNT).max_by_key(|&i| weights[i]).unwrap_or(0);
    out[largest] += (255 - assigned) as u8;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sum(w: LayerWeights) -> u32 {
        w.iter().map(|&v| u32::from(v)).sum()
    }

    #[test]
    fn dithered_paint_keeps_sum_and_reaches_full() {
        let mut w = FIRST_ONLY;
        for i in 0..2000 {
            let dither = (i as f32 * 0.618_034).fract();
            w = paint_toward(w, 2, 0.05, dither);
            assert_eq!(sum(w), 255);
        }
        assert_eq!(w, [0, 0, 255, 0]);
    }

    #[test]
    fn tiny_amounts_without_dither_change_nothing() {
        assert_eq!(paint_toward(FIRST_ONLY, 1, 0.001, 0.0), FIRST_ONLY);
    }

    #[test]
    fn paint_shrinks_others_proportionally() {
        let w = paint_toward([100, 100, 55, 0], 3, 0.5, 0.5);
        assert_eq!(sum(w), 255);
        assert!(w[3] >= 127);
        assert!(w[0] >= w[2] && w[1] >= w[2]);
    }

    #[test]
    fn zero_amount_is_a_no_op() {
        assert_eq!(paint_toward([10, 245, 0, 0], 0, 0.0, 0.9), [10, 245, 0, 0]);
    }

    #[test]
    fn normalize_fixes_bad_sums() {
        assert_eq!(normalize_weights([0, 0, 0, 0]), FIRST_ONLY);
        assert_eq!(sum(normalize_weights([200, 200, 7, 1])), 255);
        assert_eq!(sum(normalize_weights([1, 1, 1, 1])), 255);
    }
}
