//! What each tool does to the terrain in one frame.

use bevy::math::{Vec2, Vec3Swizzles};
use lofi_world::{
    ChunkCoord, Diagonal, MaskStack, MaterialId, NEUTRAL_TINT, Terrain, VertexRect, fbm, hash01,
};

use super::brush::{BrushSettings, CellMode, Tool, falloff};
use crate::select::Selection;

/// Metres per second at full strength and full falloff.
const RAISE_RATE: f32 = 20.0;
const NOISE_RATE: f32 = 10.0;
/// Exponential approach rates, per second at full strength.
const SMOOTH_RATE: f32 = 10.0;
const FLATTEN_RATE: f32 = 8.0;
const PAINT_RATE: f32 = 8.0;
const TINT_RATE: f32 = 6.0;
/// A cell's masks must pass at least this much for the Cells tool to act.
const CELL_MASK_CUTOFF: f32 = 0.5;

/// One frame of brush application.
pub struct Dab {
    pub center: Vec2,
    pub invert: bool,
    /// Flatten's target height.
    pub flatten_height: f32,
    pub dt: f32,
    /// Varies per dab so paint rounding doesn't repeat.
    pub seed: u32,
    /// What inverted paint erases back to.
    pub base: MaterialId,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DabOutcome {
    pub changed: bool,
    /// A chunk that had no room for the paint, so some vertices were skipped.
    pub refused: Option<ChunkCoord>,
}

/// Vertices a dab at `center` may change. For the Cells tool: the corners of
/// every cell whose centre is in the brush, plus the cell under the cursor
/// even when the brush is smaller than a cell.
pub fn footprint(terrain: &Terrain, brush: &BrushSettings, center: Vec2) -> Option<VertexRect> {
    if brush.tool != Tool::Cells {
        return terrain.vertices_near(center, brush.radius);
    }
    let (cx, cz) = terrain.cell_at(center)?;
    let under = VertexRect {
        min_x: cx,
        min_z: cz,
        max_x: cx + 1,
        max_z: cz + 1,
    };
    // Cell i's centre is at (i + 0.5) cells.
    let cells = |c: f32| c / terrain.cell_size() - 0.5;
    let size = terrain.size();
    let lo = (center - brush.radius).map(|c| cells(c).ceil().max(0.0));
    let hi = (center + brush.radius).map(cells).floor();
    let last = Vec2::new(size.cells_x() as f32 - 1.0, size.cells_z() as f32 - 1.0);
    let hi = hi.min(last);
    if lo.x > hi.x || lo.y > hi.y {
        return Some(under);
    }
    let centres = VertexRect {
        min_x: lo.x as u32,
        min_z: lo.y as u32,
        max_x: hi.x as u32 + 1,
        max_z: hi.y as u32 + 1,
    };
    Some(centres.union(under))
}

/// Rates scale with `dt`, so strokes feel the same at any frame rate.
pub fn apply_dab(
    terrain: &mut Terrain,
    brush: &BrushSettings,
    selection: &Selection,
    dab: &Dab,
    rect: VertexRect,
) -> DabOutcome {
    // Cells are on or off and weigh their masks per cell.
    if brush.tool == Tool::Cells {
        return DabOutcome {
            changed: apply_cells(terrain, brush, selection, dab, rect),
            refused: None,
        };
    }
    // Every weight before any write, so masks read the ground as it was and
    // a height mask doesn't chase its own edits.
    let weights: Vec<f32> = rect
        .iter()
        .map(|(x, z)| {
            let p = terrain.vertex_position(x, z).xz();
            let w = falloff(p.distance(dab.center), brush.radius, brush.hardness);
            if w <= 0.0 {
                return 0.0;
            }
            w * selection.weight(x, z) * brush.masks.weight(terrain, x, z)
        })
        .collect();
    let width = rect.width() as usize;
    let weight = |_: &Terrain, x: u32, z: u32| {
        weights[(z - rect.min_z) as usize * width + (x - rect.min_x) as usize]
    };
    let sign = if dab.invert { -1.0 } else { 1.0 };
    let mut changed = false;
    let mut refused = None;

    match brush.tool {
        Tool::Raise => {
            let step = RAISE_RATE * brush.strength * dab.dt * sign;
            for (x, z) in rect.iter() {
                let w = weight(terrain, x, z);
                if w > 0.0 {
                    changed |= terrain.set_height(x, z, terrain.height(x, z) + step * w);
                }
            }
        }
        Tool::Noise => {
            // Feature size follows the brush: big brush, big lumps.
            let scale = (brush.radius * 0.5).max(terrain.cell_size() * 2.0);
            let step = NOISE_RATE * brush.strength * dab.dt * sign;
            for (x, z) in rect.iter() {
                let w = weight(terrain, x, z);
                if w > 0.0 {
                    let n = fbm(terrain.vertex_position(x, z).xz() / scale);
                    changed |= terrain.set_height(x, z, terrain.height(x, z) + step * w * n);
                }
            }
        }
        Tool::Smooth => {
            let k = approach(SMOOTH_RATE * brush.strength, dab.dt);
            // Read every average before writing any, so the result doesn't
            // depend on visiting order.
            let targets: Vec<f32> = rect
                .iter()
                .map(|(x, z)| box_average(terrain, x, z))
                .collect();
            for ((x, z), target) in rect.iter().zip(targets) {
                let w = weight(terrain, x, z);
                if w > 0.0 {
                    let h = terrain.height(x, z);
                    changed |= terrain.set_height(x, z, h + (target - h) * k * w);
                }
            }
        }
        Tool::Flatten => {
            let k = approach(FLATTEN_RATE * brush.strength, dab.dt);
            for (x, z) in rect.iter() {
                let w = weight(terrain, x, z);
                if w > 0.0 {
                    let h = terrain.height(x, z);
                    changed |= terrain.set_height(x, z, h + (dab.flatten_height - h) * k * w);
                }
            }
        }
        Tool::Tint => {
            let k = approach(TINT_RATE * brush.strength, dab.dt);
            let target = if dab.invert { NEUTRAL_TINT } else { brush.tint };
            for (x, z) in rect.iter() {
                let w = weight(terrain, x, z);
                if w > 0.0 {
                    let dither = hash01(x as i32, z as i32, dab.seed);
                    changed |= terrain.tint_toward(x, z, target, k * w, dither);
                }
            }
        }
        // Handled above, and not a brush.
        Tool::Cells | Tool::Select => {}
        Tool::Paint => {
            let k = approach(PAINT_RATE * brush.strength, dab.dt);
            for (x, z) in rect.iter() {
                let w = weight(terrain, x, z);
                if w > 0.0 {
                    let dither = hash01(x as i32, z as i32, dab.seed);
                    let mut result = if dab.invert {
                        terrain.paint_vertex(x, z, dab.base, k * w, dither)
                    } else {
                        terrain.paint_vertex(x, z, brush.material, k * w, dither)
                    };
                    // A full chunk without the base can still lose the brush
                    // material to one it already has.
                    if dab.invert
                        && result.is_err()
                        && let Some(other) = erase_fallback(terrain, x, z, brush.material)
                    {
                        result = terrain.paint_vertex(x, z, other, k * w, dither);
                    }
                    match result {
                        Ok(c) => changed |= c,
                        Err(chunk) => refused = Some(chunk),
                    }
                }
            }
        }
    }
    DabOutcome { changed, refused }
}

/// Sets or clears the brush's cell flag on cells whose centre is inside the
/// brush, plus the cell under the cursor. Flags are on or off, so hardness
/// doesn't apply; masks must pass by half, averaged over the cell's corners.
fn apply_cells(
    terrain: &mut Terrain,
    brush: &BrushSettings,
    selection: &Selection,
    dab: &Dab,
    rect: VertexRect,
) -> bool {
    let cell = terrain.cell_size();
    let under = terrain.cell_at(dab.center);
    let set = !dab.invert;
    let mut changed = false;
    for cz in rect.min_z..rect.max_z {
        for cx in rect.min_x..rect.max_x {
            let centre = (Vec2::new(cx as f32, cz as f32) + 0.5) * cell;
            let inside = centre.distance(dab.center) <= brush.radius || under == Some((cx, cz));
            if !inside || !selection.contains_point(centre) {
                continue;
            }
            if brush.masks.is_active() {
                let corners = [(cx, cz), (cx + 1, cz), (cx, cz + 1), (cx + 1, cz + 1)];
                let mask = corners
                    .iter()
                    .map(|&(x, z)| brush.masks.weight(terrain, x, z))
                    .sum::<f32>()
                    / 4.0;
                if mask < CELL_MASK_CUTOFF {
                    continue;
                }
            }
            let flags = terrain.cell(cx, cz);
            let next = match brush.cell_mode {
                CellMode::Hole => flags.with_hole(set),
                CellMode::NoClutter => flags.with_no_clutter(set),
                CellMode::Unwalkable => flags.with_unwalkable(set),
                CellMode::Diagonal(d) => flags.with_diagonal(if set { d } else { Diagonal::Auto }),
            };
            changed |= terrain.set_cell(cx, cz, next);
        }
    }
    changed
}

/// Below this a mask's soft edge is ignored by auto-paint, so faint tails
/// don't claim one of a chunk's four slots for a trace of paint.
const AUTO_PAINT_CUTOFF: f32 = 0.1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AutoPaintOutcome {
    pub changed: bool,
    /// Vertices skipped because their chunk had no room.
    pub refused: usize,
}

/// Paints `material` over `rect` in one pass, as strongly as the masks and
/// selection pass there.
pub fn auto_paint(
    terrain: &mut Terrain,
    masks: &MaskStack,
    selection: &Selection,
    material: MaterialId,
    rect: VertexRect,
) -> AutoPaintOutcome {
    let weights: Vec<f32> = rect
        .iter()
        .map(|(x, z)| selection.weight(x, z) * masks.weight(terrain, x, z))
        .collect();
    let mut outcome = AutoPaintOutcome::default();
    for ((x, z), w) in rect.iter().zip(weights) {
        if w < AUTO_PAINT_CUTOFF {
            continue;
        }
        let dither = hash01(x as i32, z as i32, 0x0a17_0a17);
        match terrain.paint_vertex(x, z, material, w, dither) {
            Ok(changed) => outcome.changed |= changed,
            Err(_) => outcome.refused += 1,
        }
    }
    outcome
}

/// Whether flags set by `mode` change the mesh (and so the water).
pub const fn cell_mode_changes_shape(mode: CellMode) -> bool {
    matches!(mode, CellMode::Hole | CellMode::Diagonal(_))
}

/// What erasing paints toward when the base won't fit: the strongest other
/// material at the vertex, else any other its chunk blends.
fn erase_fallback(terrain: &Terrain, x: u32, z: u32, erasing: MaterialId) -> Option<MaterialId> {
    let here = terrain.paint_at(x, z);
    let strongest = here
        .iter()
        .filter(|&(m, _)| m != erasing)
        .max_by_key(|&(_, w)| w)
        .map(|(m, _)| m);
    strongest.or_else(|| {
        let (chunk, _, _) = terrain.size().owner(x, z);
        terrain
            .chunk_paint(chunk)
            .materials()
            .find(|&m| m != erasing)
    })
}

/// Fraction of the remaining distance covered in `dt` at `rate` per second.
fn approach(rate: f32, dt: f32) -> f32 {
    1.0 - (-rate * dt).exp()
}

/// Mean of the 3x3 neighbourhood, clamped at the map edge.
fn box_average(terrain: &Terrain, x: u32, z: u32) -> f32 {
    let size = terrain.size();
    let mut sum = 0.0;
    let mut count = 0.0;
    for nz in z.saturating_sub(1)..=(z + 1).min(size.cells_z()) {
        for nx in x.saturating_sub(1)..=(x + 1).min(size.cells_x()) {
            sum += terrain.height(nx, nz);
            count += 1.0;
        }
    }
    sum / count
}

#[cfg(test)]
mod tests {
    use super::*;
    use lofi_world::{GridSize, VertexPaint};

    const CENTER: Vec2 = Vec2::new(32.0, 32.0);
    /// Smaller than a cell.
    const MIN_RADIUS_FOR_TEST: f32 = 0.5;
    const GRASS: MaterialId = MaterialId(0);
    const ROCK: MaterialId = MaterialId(2);

    fn terrain() -> Terrain {
        Terrain::flat(GridSize::new(1, 1).expect("valid size"), 2.0, 0.0, GRASS)
    }

    fn brush(tool: Tool) -> BrushSettings {
        BrushSettings {
            tool,
            radius: 8.0,
            strength: 1.0,
            hardness: 0.0,
            material: ROCK,
            ..BrushSettings::default()
        }
    }

    fn dab_outcome(
        t: &mut Terrain,
        b: &BrushSettings,
        invert: bool,
        flatten_height: f32,
    ) -> DabOutcome {
        dab_within(t, b, &Selection::default(), invert, flatten_height)
    }

    fn dab_within(
        t: &mut Terrain,
        b: &BrushSettings,
        selection: &Selection,
        invert: bool,
        flatten_height: f32,
    ) -> DabOutcome {
        let rect = footprint(t, b, CENTER).expect("on map");
        let d = Dab {
            center: CENTER,
            invert,
            flatten_height,
            dt: 0.1,
            seed: 7,
            base: GRASS,
        };
        apply_dab(t, b, selection, &d, rect)
    }

    fn dab(t: &mut Terrain, b: &BrushSettings, invert: bool, flatten_height: f32) -> bool {
        dab_outcome(t, b, invert, flatten_height).changed
    }

    #[test]
    fn dabs_that_change_nothing_say_so() {
        let mut t = terrain();
        assert!(
            !dab(&mut t, &brush(Tool::Flatten), false, 0.0),
            "already flat"
        );
        assert!(
            !dab(&mut t, &brush(Tool::Smooth), false, 0.0),
            "already smooth"
        );
        assert!(dab(&mut t, &brush(Tool::Raise), false, 0.0));
    }

    #[test]
    fn raise_peaks_at_the_centre_and_stays_inside() {
        let mut t = terrain();
        let b = brush(Tool::Raise);
        dab(&mut t, &b, false, 0.0);
        assert!(t.height(16, 16) > t.height(18, 16));
        assert!(t.height(18, 16) > 0.0);
        assert_eq!(t.height(21, 16), 0.0, "outside the radius");
    }

    #[test]
    fn invert_lowers() {
        let mut t = terrain();
        dab(&mut t, &brush(Tool::Raise), true, 0.0);
        assert!(t.height(16, 16) < 0.0);
    }

    #[test]
    fn smooth_pulls_a_spike_down() {
        let mut t = terrain();
        t.set_height(16, 16, 10.0);
        dab(&mut t, &brush(Tool::Smooth), false, 0.0);
        assert!(t.height(16, 16) < 10.0);
        assert!(t.height(17, 16) > 0.0);
    }

    #[test]
    fn flatten_approaches_target_without_overshoot() {
        let mut t = terrain();
        let b = brush(Tool::Flatten);
        for _ in 0..50 {
            dab(&mut t, &b, false, 5.0);
        }
        let h = t.height(16, 16);
        assert!(h > 4.9 && h <= 5.0);
    }

    #[test]
    fn paint_moves_toward_the_material() {
        let mut t = terrain();
        dab(&mut t, &brush(Tool::Paint), false, 0.0);
        assert!(t.paint_at(16, 16).weight_of(ROCK) > 0);
        assert_eq!(t.paint_at(0, 0), VertexPaint::solid(GRASS));
    }

    #[test]
    fn inverted_paint_erases_to_the_base() {
        let mut t = terrain();
        let b = brush(Tool::Paint);
        for _ in 0..30 {
            dab(&mut t, &b, false, 0.0);
        }
        assert_eq!(t.paint_at(16, 16), VertexPaint::solid(ROCK));
        for _ in 0..30 {
            dab(&mut t, &b, true, 0.0);
        }
        assert_eq!(t.paint_at(16, 16), VertexPaint::solid(GRASS));
    }

    #[test]
    fn paint_into_a_full_chunk_is_refused() {
        let mut t = terrain();
        for (x, m) in [(1, 1), (2, 3), (3, 4)] {
            t.set_vertex_paint(x, 1, VertexPaint::solid(MaterialId(m)))
                .expect("room");
        }
        let outcome = dab_outcome(&mut t, &brush(Tool::Paint), false, 0.0);
        assert!(!outcome.changed);
        assert!(outcome.refused.is_some());
    }

    #[test]
    fn erasing_in_a_full_chunk_without_the_base_falls_back() {
        let mut t = terrain();
        // Four materials, none of them the base (grass).
        let others = [ROCK, MaterialId(1), MaterialId(3), MaterialId(4)];
        for (x, z) in t.size().full_rect().iter() {
            let m = if x < 16 { MaterialId(1) } else { ROCK };
            t.set_vertex_paint(x, z, VertexPaint::solid(m))
                .expect("room");
        }
        t.set_vertex_paint(0, 30, VertexPaint::solid(MaterialId(3)))
            .expect("room");
        t.set_vertex_paint(1, 30, VertexPaint::solid(MaterialId(4)))
            .expect("room");
        let before = t.paint_at(16, 16).weight_of(ROCK);
        let outcome = dab_outcome(&mut t, &brush(Tool::Paint), true, 0.0);
        assert!(outcome.changed, "erased something");
        assert!(t.paint_at(16, 16).weight_of(ROCK) < before);
        assert!(
            t.chunk_paint(ChunkCoord { x: 0, z: 0 })
                .materials()
                .all(|m| others.contains(&m))
        );
    }

    #[test]
    fn the_cells_tool_reaches_the_cell_under_a_tiny_brush() {
        let mut t = terrain();
        let b = BrushSettings {
            radius: MIN_RADIUS_FOR_TEST,
            ..brush(Tool::Cells)
        };
        assert!(dab(&mut t, &b, false, 0.0));
        // CENTER (32, 32) m is the corner of cells 15 and 16; it lies in 16.
        assert!(t.cell(16, 16).hole());
        assert!(!t.cell(14, 14).hole());
        assert!(dab(&mut t, &b, true, 0.0), "Ctrl fills it again");
        assert!(!t.cell(16, 16).hole());
    }

    #[test]
    fn cells_follow_their_centres_inside_the_radius() {
        let mut t = terrain();
        let b = BrushSettings {
            cell_mode: CellMode::Diagonal(Diagonal::Anti),
            ..brush(Tool::Cells)
        };
        dab(&mut t, &b, false, 0.0);
        // Radius 8 m: cell (13, 15) has its centre at (27, 31), 5.1 m away.
        assert_eq!(t.cell(13, 15).diagonal(), Diagonal::Anti);
        // Cell (11, 15)'s centre is 9 m away.
        assert_eq!(t.cell(11, 15).diagonal(), Diagonal::Auto);
    }

    #[test]
    fn every_cell_with_its_centre_in_the_brush_is_reached() {
        let t = terrain();
        for (centre, radius) in [
            (Vec2::new(20.3, 20.3), 3.0),
            (Vec2::new(20.7, 33.1), 5.0),
            (Vec2::new(0.4, 63.7), 4.0),
        ] {
            let b = BrushSettings {
                radius,
                ..brush(Tool::Cells)
            };
            let rect = footprint(&t, &b, centre).expect("on map");
            for cz in 0..32 {
                for cx in 0..32 {
                    let c = (Vec2::new(cx as f32, cz as f32) + 0.5) * 2.0;
                    if c.distance(centre) <= radius {
                        assert!(
                            cx >= rect.min_x
                                && cx < rect.max_x
                                && cz >= rect.min_z
                                && cz < rect.max_z,
                            "cell {cx},{cz} missed for {centre} r {radius}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_selection_fences_the_brush() {
        let mut t = terrain();
        // Only x ≥ 32 m is selected.
        let selection = Selection::rect(&t, Vec2::new(32.0, 0.0), Vec2::new(64.0, 64.0));
        dab_within(&mut t, &brush(Tool::Raise), &selection, false, 0.0);
        assert!(t.height(17, 16) > 0.0);
        assert_eq!(t.height(15, 16), 0.0);
    }

    #[test]
    fn masks_read_the_ground_before_the_dab() {
        let mut t = terrain();
        // Raising can't lift ground out of a "below 0.01 m" mask mid-dab, so
        // every vertex the mask passed at the start rises together.
        let b = BrushSettings {
            masks: MaskStack(vec![lofi_world::Mask {
                fade: 0.0,
                ..lofi_world::Mask::new(lofi_world::MaskKind::Height {
                    min: -1.0,
                    max: 0.01,
                })
            }]),
            ..brush(Tool::Raise)
        };
        dab(&mut t, &b, false, 0.0);
        assert!(t.height(16, 16) > 0.01);
        assert!(t.height(17, 16) > 0.01);
        // A second dab finds everything above the mask's band.
        let before = t.height(16, 16);
        assert!(!dab(&mut t, &b, false, 0.0));
        assert_eq!(t.height(16, 16), before);
    }

    #[test]
    fn auto_paint_follows_masks_and_scope() {
        let mut t = terrain();
        for (x, z) in t.size().full_rect().iter() {
            t.set_height(x, z, x as f32);
        }
        let high = MaskStack(vec![lofi_world::Mask {
            fade: 0.0,
            ..lofi_world::Mask::new(lofi_world::MaskKind::Height {
                min: 20.0,
                max: 100.0,
            })
        }]);
        let full = t.size().full_rect();
        let outcome = auto_paint(&mut t, &high, &Selection::default(), ROCK, full);
        assert!(outcome.changed && outcome.refused == 0);
        assert_eq!(t.paint_at(25, 3), VertexPaint::solid(ROCK));
        assert_eq!(t.paint_at(10, 3), VertexPaint::solid(GRASS));

        // Only the selected part of the masked area.
        let mut t2 = terrain();
        for (x, z) in t2.size().full_rect().iter() {
            t2.set_height(x, z, x as f32);
        }
        let selection = Selection::rect(&t2, Vec2::ZERO, Vec2::new(64.0, 20.0));
        let rect = selection.vertex_rect().expect("selected");
        auto_paint(&mut t2, &high, &selection, ROCK, rect);
        assert_eq!(t2.paint_at(25, 3), VertexPaint::solid(ROCK));
        assert_eq!(t2.paint_at(25, 20), VertexPaint::solid(GRASS), "outside");
    }
}
