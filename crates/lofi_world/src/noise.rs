//! Cheap deterministic noise for brushes, dithering and generators.

use bevy_math::Vec2;

const OCTAVES: u32 = 4;

/// Fractal value noise, roughly -1..1.
///
/// Fixed in space: a brush held still grows the same features taller rather
/// than piling up random fuzz.
#[must_use]
pub fn fbm(p: Vec2) -> f32 {
    let mut sum = 0.0;
    let mut norm = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = 1.0;
    for octave in 0..OCTAVES {
        sum += amplitude * value_noise(p * frequency, octave);
        norm += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    sum / norm
}

/// Uniform 0..1 per integer lattice point and seed.
#[must_use]
pub fn hash01(x: i32, z: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343)
        ^ (z as u32).wrapping_mul(0xd816_3841)
        ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    // Top 24 bits, so the result is exactly representable and below 1.
    (h >> 8) as f32 / (1u32 << 24) as f32
}

fn value_noise(p: Vec2, seed: u32) -> f32 {
    let cell = p.floor();
    let f = p - cell;
    let u = f * f * (3.0 - 2.0 * f);
    let (x, z) = (cell.x as i32, cell.y as i32);
    let corner = |dx, dz| hash01(x + dx, z + dz, seed) * 2.0 - 1.0;
    let bottom = corner(0, 0) + (corner(1, 0) - corner(0, 0)) * u.x;
    let top = corner(0, 1) + (corner(1, 1) - corner(0, 1)) * u.x;
    bottom + (top - bottom) * u.y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fbm_is_bounded_deterministic_and_varied() {
        let samples: Vec<f32> = (0..500)
            .map(|i| fbm(Vec2::new(i as f32 * 0.37, i as f32 * -0.21)))
            .collect();
        assert!(samples.iter().all(|v| (-1.0..=1.0).contains(v)));
        assert_eq!(fbm(Vec2::new(3.3, 4.4)), fbm(Vec2::new(3.3, 4.4)));
        let (lo, hi) = samples
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
        assert!(hi - lo > 0.5);
    }

    #[test]
    fn hash_is_in_unit_range() {
        for i in -100..100 {
            let v = hash01(i, i * 7, 3);
            assert!((0.0..1.0).contains(&v));
        }
    }
}
