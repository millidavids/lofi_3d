//! A ray against the ground: a walk over the cells under the ray, watching
//! for the ray's height to drop through the surface's.
//!
//! No triangle test, so no edge tolerance to outgrow: the surface is one
//! continuous function, and a ray that is above it at one point and below it
//! at the next has crossed it. A fixed-step march can step over a spike, and
//! per-triangle tests let long rays slip between triangles; this does
//! neither.

use bevy_math::{Vec2, Vec3, Vec3Swizzles};

use super::cell::{CellShape, Plane, cell_plane};
use super::query::{Ground, GroundHit, Holes, RayTrace, walkable};

/// Below this, a direction component counts as parallel to the axis.
const PARALLEL: f32 = 1e-8;
/// How clear of a cell's highest or lowest corner a ray must pass for the
/// cell to be skipped without looking at how it's cut, metres.
const CLEARANCE: f32 = 1e-2;
/// A ray starting this close under the surface starts on it: a point taken
/// from the ground and cast from again must hit, rounding or not.
const ON_SURFACE: f32 = 1e-3;

pub(super) fn trace<G: Ground + ?Sized>(
    ground: &G,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
    holes: Holes,
) -> RayTrace {
    let dir = direction.normalize_or_zero();
    if dir == Vec3::ZERO || !origin.is_finite() || max_distance.is_nan() {
        return RayTrace::Clear;
    }
    let size = ground.cell_size();
    let (cells_x, cells_z) = ground.cell_counts();
    if cells_x == 0 || cells_z == 0 {
        return RayTrace::Clear;
    }
    let extent = Vec2::new(cells_x as f32, cells_z as f32) * size;
    let Some((t_enter, t_exit)) = clip_to_footprint(origin, dir, extent, max_distance) else {
        return RayTrace::Clear;
    };

    let step = |d: f32| {
        if d > PARALLEL {
            1
        } else if d < -PARALLEL {
            -1
        } else {
            0
        }
    };
    let (step_x, step_z) = (step(dir.x), step(dir.z));
    let start = origin + dir * t_enter;
    let first = |v: f32, count: u32| ((v / size).floor() as i64).clamp(0, i64::from(count) - 1);
    let (mut cx, mut cz) = start_cell(
        ground,
        (first(start.x, cells_x), first(start.z, cells_z)),
        (start.x, start.z),
        (step_x, step_z),
    );
    // Where the ray leaves cell `c` along one axis. From the cell's index
    // each time, not accumulated, so a long ray doesn't drift.
    let leaves = |c: i64, step: i64, o: f32, d: f32| {
        if step == 0 {
            f32::INFINITY
        } else {
            ((c + i64::from(step > 0)) as f32 * size - o) / d
        }
    };
    let slack = ON_SURFACE + 2.0 * f32::EPSILON * origin.abs().max_element();

    // Whether the ray was above the surface where the last cell left off.
    // `None` until the first cell is looked at.
    let mut above: Option<bool> = None;
    let mut t_in = t_enter;
    // A ray crosses at most one cell per row and per column.
    for _ in 0..=cells_x + cells_z + 1 {
        let t_x = leaves(cx, step_x, origin.x, dir.x);
        let t_z = leaves(cz, step_z, origin.z, dir.z);
        let t_out = t_x.min(t_z).min(t_exit);
        let (a, b) = (t_in.min(t_out), t_out.max(t_in));
        let cell = (cx as u32, cz as u32);

        let Some(corners) = ground.cell_corners(cell.0, cell.1) else {
            return RayTrace::Unknown { distance: a };
        };
        let top = corners.into_iter().fold(f32::NEG_INFINITY, f32::max);
        let bottom = corners.into_iter().fold(f32::INFINITY, f32::min);
        // An endless ray straight down: far enough to be under the cell.
        let b = if b.is_finite() {
            b
        } else if dir.y < 0.0 {
            a.max((origin.y - bottom + 1.0) / -dir.y)
        } else {
            a
        };
        let (y_a, y_b) = (origin.y + dir.y * a, origin.y + dir.y * b);

        // Most cells along a ray are passed well clear; skip them before
        // asking how the cell is cut, which costs the editor's terrain most.
        if y_a.min(y_b) > top + CLEARANCE {
            above = Some(true);
        } else if y_a.max(y_b) < bottom - CLEARANCE {
            above = Some(false);
        } else {
            let Some(shape) = ground.cell_shape(cell.0, cell.1) else {
                return RayTrace::Unknown { distance: a };
            };
            let open = shape.hole && holes == Holes::Open;
            let corner = Vec2::new(cx as f32, cz as f32) * size;
            // The ray's height over the surface, and the surface there.
            let over = |t: f32| {
                let p = origin + dir * t;
                let uv = ((p.xz() - corner) / size).clamp(Vec2::ZERO, Vec2::ONE);
                let plane = cell_plane(corners, shape.main_diagonal, uv.x, uv.y);
                (p.y - plane.height, plane)
            };
            // The ray is on one triangle up to the cell's cut and on the
            // other after it; over each, its height above is a straight line.
            let cut = if shape.main_diagonal {
                let closing = dir.x - dir.z;
                (closing.abs() > PARALLEL)
                    .then(|| ((corner.x - corner.y) - (origin.x - origin.z)) / closing)
            } else {
                let closing = dir.x + dir.z;
                (closing.abs() > PARALLEL)
                    .then(|| ((corner.x + corner.y + size) - (origin.x + origin.z)) / closing)
            };
            let cut = cut.filter(|t| *t > a && *t < b).unwrap_or(a);

            let mut prev = (a, over(a).0);
            match above {
                // The ray starts here. A hair under the surface is on it.
                None if prev.1 < 0.0 && prev.1 >= -slack => prev.1 = 0.0,
                // The last cell said above and this one says below: the two
                // round differently along their shared edge, and the ray
                // crossed on it.
                Some(true) if prev.1 < 0.0 && !open => {
                    return hit(size, origin, dir, a, (cell, shape), &over);
                }
                // Coming up from underneath, touching isn't crossing.
                Some(false) if prev.1 == 0.0 => prev.1 = -f32::MIN_POSITIVE,
                _ => {}
            }
            for t in [cut, b] {
                let now = (t, over(t).0);
                if prev.1 >= 0.0 && now.1 < 0.0 && !open {
                    let along = prev.1 / (prev.1 - now.1);
                    let distance = prev.0 + (now.0 - prev.0) * along;
                    if distance.is_finite() {
                        return hit(size, origin, dir, distance, (cell, shape), &over);
                    }
                }
                prev = now;
            }
            // Through an open hole the ray is underground all the same.
            above = Some(prev.1 >= 0.0);
        }

        if t_out >= t_exit {
            return RayTrace::Clear;
        }
        if t_x <= t_z {
            cx += step_x;
        } else {
            cz += step_z;
        }
        if cx < 0 || cz < 0 || cx >= i64::from(cells_x) || cz >= i64::from(cells_z) {
            return RayTrace::Clear;
        }
        t_in = t_out;
    }
    RayTrace::Clear
}

fn hit(
    cell_size: f32,
    origin: Vec3,
    dir: Vec3,
    distance: f32,
    (cell, shape): ((u32, u32), CellShape),
    over: &impl Fn(f32) -> (f32, Plane),
) -> RayTrace {
    let distance = distance.max(0.0);
    let (_, plane) = over(distance);
    let normal = Vec3::new(-plane.rise_x, cell_size, -plane.rise_z).normalize();
    RayTrace::Hit(GroundHit {
        distance,
        point: origin + dir * distance,
        normal,
        cell,
        walkable: walkable(normal, shape),
    })
}

/// The cell a ray starts in. A ray running along a grid line is on the edge
/// of the cells on both sides of it, and at a streaming border only one of
/// them may be loaded: take the one that is, as `ground_at` does.
fn start_cell<G: Ground + ?Sized>(
    ground: &G,
    (cx, cz): (i64, i64),
    (x, z): (f32, f32),
    (step_x, step_z): (i64, i64),
) -> (i64, i64) {
    let size = ground.cell_size();
    let across =
        |c: i64, at: f32, step: i64| (step == 0 && c > 0 && at == c as f32 * size).then_some(c - 1);
    let xs = [Some(cx), across(cx, x, step_x)];
    let zs = [Some(cz), across(cz, z, step_z)];
    zs.into_iter()
        .flatten()
        .flat_map(|cz| xs.into_iter().flatten().map(move |cx| (cx, cz)))
        .find(|&(cx, cz)| ground.cell_corners(cx as u32, cz as u32).is_some())
        .unwrap_or((cx, cz))
}

/// The ray's parameter range over the map's XZ rectangle (slab test).
fn clip_to_footprint(
    origin: Vec3,
    dir: Vec3,
    extent: Vec2,
    max_distance: f32,
) -> Option<(f32, f32)> {
    let mut t0 = 0.0_f32;
    let mut t1 = max_distance;
    for (o, d, hi) in [(origin.x, dir.x, extent.x), (origin.z, dir.z, extent.y)] {
        if d.abs() <= PARALLEL {
            if o < 0.0 || o > hi {
                return None;
            }
        } else {
            let a = -o / d;
            let b = (hi - o) / d;
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b));
        }
    }
    (t0 <= t1).then_some((t0, t1))
}
