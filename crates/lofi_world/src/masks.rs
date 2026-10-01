//! Masks that limit where a brush or auto-paint acts, by the ground's own
//! shape: height, slope, curvature and noise.
//!
//! A stack multiplies its masks, so "steep and high" is two masks.

use bevy_math::Vec3Swizzles;

use crate::noise::fbm;
use crate::terrain::Terrain;

/// Steeper than this, a character slides off. WoW's limit was about 50°.
pub const WALKABLE_SLOPE_DEGREES: f32 = 50.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MaskKind {
    /// Metres.
    Height { min: f32, max: f32 },
    /// Degrees from level.
    Slope { min: f32, max: f32 },
    /// [`Terrain::curvature`]: positive in hollows, negative on ridges.
    Curvature { min: f32, max: f32 },
    /// Fractal noise, -1..1, at `scale` metres per feature. Passes above
    /// `threshold`.
    Noise { scale: f32, threshold: f32 },
}

impl MaskKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Height { .. } => "Height",
            Self::Slope { .. } => "Slope",
            Self::Curvature { .. } => "Curvature",
            Self::Noise { .. } => "Noise",
        }
    }

    /// Reasonable starting values for a new mask of each kind.
    pub const DEFAULTS: [Self; 4] = [
        Self::Height {
            min: 0.0,
            max: 20.0,
        },
        Self::Slope {
            min: WALKABLE_SLOPE_DEGREES,
            max: 90.0,
        },
        Self::Curvature {
            min: 0.02,
            max: 10.0,
        },
        Self::Noise {
            scale: 20.0,
            threshold: 0.0,
        },
    ];
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mask {
    pub kind: MaskKind,
    /// Width of the soft edge outside the range, in the kind's units.
    pub fade: f32,
    pub invert: bool,
    pub enabled: bool,
}

impl Mask {
    #[must_use]
    pub const fn new(kind: MaskKind) -> Self {
        let fade = match kind {
            MaskKind::Height { .. } => 2.0,
            MaskKind::Slope { .. } => 5.0,
            MaskKind::Curvature { .. } => 0.02,
            MaskKind::Noise { .. } => 0.1,
        };
        Self {
            kind,
            fade,
            invert: false,
            enabled: true,
        }
    }

    /// 0..1 at a vertex.
    #[must_use]
    pub fn weight(&self, terrain: &Terrain, x: u32, z: u32) -> f32 {
        let w = match self.kind {
            MaskKind::Height { min, max } => band(terrain.height(x, z), min, max, self.fade),
            MaskKind::Slope { min, max } => {
                let degrees = terrain.normal(x, z).y.clamp(-1.0, 1.0).acos().to_degrees();
                band(degrees, min, max, self.fade)
            }
            MaskKind::Curvature { min, max } => band(terrain.curvature(x, z), min, max, self.fade),
            MaskKind::Noise { scale, threshold } => {
                let p = terrain.vertex_position(x, z).xz();
                let n = fbm(p / scale.max(0.01));
                smoothstep(threshold - self.fade, threshold + self.fade, n)
            }
        };
        if self.invert { 1.0 - w } else { w }
    }
}

/// Masks applied together: their weights multiply. Disabled masks are
/// skipped; an empty stack passes everything.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaskStack(pub Vec<Mask>);

impl MaskStack {
    #[must_use]
    pub fn weight(&self, terrain: &Terrain, x: u32, z: u32) -> f32 {
        let mut w = 1.0;
        for mask in self.0.iter().filter(|m| m.enabled) {
            w *= mask.weight(terrain, x, z);
            if w <= 0.0 {
                return 0.0;
            }
        }
        w
    }

    /// Whether any mask is switched on.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.0.iter().any(|m| m.enabled)
    }
}

/// 1 inside `min..=max`, easing to 0 over `fade` beyond either end.
fn band(value: f32, min: f32, max: f32, fade: f32) -> f32 {
    let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
    if value < lo {
        smoothstep(lo - fade, lo, value)
    } else if value > hi {
        1.0 - smoothstep(hi, hi + fade, value)
    } else {
        1.0
    }
}

/// Hermite step; a zero-width edge is a hard step at `edge1`.
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge1 <= edge0 {
        return if x >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::GridSize;
    use crate::materials::MaterialId;

    /// A ramp rising 1 m per 1 m cell along x: 45° everywhere inside.
    fn ramp() -> Terrain {
        let size = GridSize::new(1, 1).expect("valid size");
        let mut t = Terrain::flat(size, 1.0, 0.0, MaterialId(0));
        for (x, z) in size.full_rect().iter() {
            t.set_height(x, z, x as f32);
        }
        t
    }

    fn mask(kind: MaskKind, fade: f32) -> Mask {
        Mask {
            fade,
            ..Mask::new(kind)
        }
    }

    #[test]
    fn height_band_with_soft_edges() {
        let t = ramp();
        let m = mask(
            MaskKind::Height {
                min: 10.0,
                max: 20.0,
            },
            4.0,
        );
        assert_eq!(m.weight(&t, 15, 3), 1.0);
        assert_eq!(m.weight(&t, 10, 3), 1.0);
        assert_eq!(m.weight(&t, 5, 3), 0.0);
        assert_eq!(m.weight(&t, 25, 3), 0.0);
        let edge = m.weight(&t, 8, 3);
        assert!(edge > 0.0 && edge < 1.0, "{edge}");
    }

    #[test]
    fn slope_reads_degrees() {
        let t = ramp();
        let steep = mask(
            MaskKind::Slope {
                min: 40.0,
                max: 50.0,
            },
            0.0,
        );
        let level = mask(
            MaskKind::Slope {
                min: 0.0,
                max: 10.0,
            },
            0.0,
        );
        assert_eq!(steep.weight(&t, 16, 16), 1.0);
        assert_eq!(level.weight(&t, 16, 16), 0.0);
    }

    #[test]
    fn curvature_finds_hollows() {
        let size = GridSize::new(1, 1).expect("valid size");
        let mut t = Terrain::flat(size, 2.0, 0.0, MaterialId(0));
        t.set_height(10, 10, -3.0);
        t.set_height(20, 20, 3.0);
        let hollow = mask(
            MaskKind::Curvature {
                min: 0.5,
                max: 100.0,
            },
            0.0,
        );
        assert_eq!(hollow.weight(&t, 10, 10), 1.0);
        assert_eq!(hollow.weight(&t, 20, 20), 0.0);
        assert_eq!(hollow.weight(&t, 5, 5), 0.0, "flat ground");
    }

    #[test]
    fn invert_and_stack_multiply() {
        let t = ramp();
        let low = mask(
            MaskKind::Height {
                min: 0.0,
                max: 10.0,
            },
            0.0,
        );
        let not_low = Mask {
            invert: true,
            ..low
        };
        assert_eq!(not_low.weight(&t, 5, 0), 0.0);
        assert_eq!(not_low.weight(&t, 15, 0), 1.0);

        let mut stack = MaskStack(vec![
            low,
            mask(
                MaskKind::Height {
                    min: 5.0,
                    max: 30.0,
                },
                0.0,
            ),
        ]);
        assert_eq!(stack.weight(&t, 7, 0), 1.0);
        assert_eq!(stack.weight(&t, 3, 0), 0.0);
        stack.0[0].enabled = false;
        assert_eq!(stack.weight(&t, 20, 0), 1.0, "disabled masks pass");
        assert!(MaskStack::default().weight(&t, 3, 0) == 1.0);
    }

    #[test]
    fn noise_splits_the_ground() {
        let size = GridSize::new(2, 2).expect("valid size");
        let t = Terrain::flat(size, 2.0, 0.0, MaterialId(0));
        let m = mask(
            MaskKind::Noise {
                scale: 10.0,
                threshold: 0.0,
            },
            0.0,
        );
        let on = size
            .full_rect()
            .iter()
            .filter(|&(x, z)| m.weight(&t, x, z) > 0.5)
            .count();
        let share = on as f32 / size.vertex_count() as f32;
        assert!((0.2..0.8).contains(&share), "{share}");
    }

    #[test]
    fn reversed_ranges_still_work() {
        let t = ramp();
        let m = mask(
            MaskKind::Height {
                min: 20.0,
                max: 10.0,
            },
            0.0,
        );
        assert_eq!(m.weight(&t, 15, 0), 1.0);
    }
}
