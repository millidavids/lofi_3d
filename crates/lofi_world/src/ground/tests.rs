use bevy_math::{Vec2, Vec3, Vec3Swizzles};

use super::*;
use crate::cells::CellFlags;
use crate::grid::{CHUNK_CELLS, ChunkCoord, GridSize};
use crate::materials::MaterialId;
use crate::noise::{fbm, hash01};
use crate::terrain::Terrain;

fn flat(height: f32) -> Terrain {
    let size = GridSize::new(2, 2).expect("valid size");
    Terrain::flat(size, 2.0, height, MaterialId(0))
}

/// Rolling hills with sharp detail over 3×2 chunks, and a few holes.
fn hills() -> Terrain {
    let size = GridSize::new(3, 2).expect("valid size");
    let mut t = Terrain::flat(size, 2.0, 0.0, MaterialId(0));
    for (x, z) in size.full_rect().iter() {
        let p = Vec2::new(x as f32, z as f32) * 2.0;
        t.set_height(x, z, fbm(p / 40.0) * 30.0 + fbm(p / 5.0) * 4.0);
    }
    for (cx, cz) in [(5, 10), (6, 10), (40, 33), (95, 63), (0, 0)] {
        t.set_cell(cx, cz, CellFlags::default().with_hole(true));
    }
    t
}

/// A cell's two triangles, relative to its (x, z) corner, counter-clockwise
/// seen from above: the two `Terrain::cell_triangles` names.
fn triangles(cell: &GroundCell, size: f32) -> [[Vec3; 3]; 2] {
    let [ha, hb, hc, hd] = cell.corners;
    let a = Vec3::new(0.0, ha, 0.0);
    let b = Vec3::new(size, hb, 0.0);
    let c = Vec3::new(0.0, hc, size);
    let d = Vec3::new(size, hd, size);
    if cell.shape.main_diagonal {
        [[a, c, d], [a, d, b]]
    } else {
        [[a, c, b], [b, c, d]]
    }
}

/// Möller–Trumbore, front faces only: the distance along the ray. Not how
/// the ray walk works, which is the point: a second opinion.
fn hit_triangle(origin: Vec3, dir: Vec3, [v0, v1, v2]: [Vec3; 3]) -> Option<f32> {
    let (e1, e2) = (v1 - v0, v2 - v0);
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det <= f32::EPSILON * e1.length() * e2.length() {
        return None;
    }
    let to_origin = origin - v0;
    let u = to_origin.dot(p) / det;
    let q = to_origin.cross(e1);
    let v = dir.dot(q) / det;
    if u < -1e-4 || v < -1e-4 || u + v > 1.0001 {
        return None;
    }
    let distance = e2.dot(q) / det;
    (distance >= 0.0).then_some(distance)
}

/// The nearest front-face hit over every triangle the ground has loaded.
pub(crate) fn brute_force<G: Ground>(
    t: &G,
    origin: Vec3,
    dir: Vec3,
    max: f32,
    holes: Holes,
) -> Option<f32> {
    let (cells_x, cells_z) = t.cell_counts();
    let size = t.cell_size();
    let mut nearest: Option<f32> = None;
    for cz in 0..cells_z {
        for cx in 0..cells_x {
            let Some(cell) = t.ground_cell(cx, cz) else {
                continue;
            };
            if cell.shape.hole && holes == Holes::Open {
                continue;
            }
            let local = origin - Vec3::new(cx as f32 * size, 0.0, cz as f32 * size);
            for tri in triangles(&cell, size) {
                if let Some(d) = hit_triangle(local, dir, tri)
                    && d <= max
                    && nearest.is_none_or(|n| d < n)
                {
                    nearest = Some(d);
                }
            }
        }
    }
    nearest
}

#[test]
fn rays_find_what_checking_every_triangle_finds() {
    let t = hills();
    let extent = t.extent();
    let r = |i: i32, k: i32| hash01(i, k, 77);
    let mut hits = 0;
    for i in 0..3000 {
        // From above or beside the map, in every direction but mostly down.
        let origin = Vec3::new(
            (r(i, 0) * 1.4 - 0.2) * extent.x,
            r(i, 1) * 80.0 - 10.0,
            (r(i, 2) * 1.4 - 0.2) * extent.y,
        );
        let dir = Vec3::new(r(i, 3) - 0.5, r(i, 4) * 0.6 - 0.5, r(i, 5) - 0.5).normalize();
        let holes = if i % 2 == 0 {
            Holes::Open
        } else {
            Holes::Solid
        };
        let found = t.cast_ray(origin, dir, 400.0, holes).map(|h| h.distance);
        let expected = brute_force(&t, origin, dir, 400.0, holes);
        match (found, expected) {
            (Some(a), Some(b)) => {
                assert!((a - b).abs() < 1e-3, "ray {i}: {a} vs {b}");
                hits += 1;
            }
            (None, None) => {}
            other => panic!("ray {i} from {origin} along {dir}: {other:?}"),
        }
    }
    assert!(hits > 800, "only {hits} rays hit; the test isn't testing");
}

/// The triangle test above is the same one the walk uses, so it can't see a
/// ray slipping between two triangles. This can: a ray from above the
/// ground to below it must hit, on the surface, having been above it all
/// the way there.
#[test]
fn a_ray_from_above_the_ground_to_below_it_always_hits() {
    let mut t = hills();
    for (cx, cz) in [(5, 10), (6, 10), (40, 33), (95, 63), (0, 0)] {
        t.set_cell(cx, cz, CellFlags::default());
    }
    let extent = t.extent();
    let r = |i: i32, k: i32| hash01(i, k, 91);
    for i in 0..4000 {
        let mut a_xz = Vec2::new(r(i, 0), r(i, 1)) * extent;
        let mut b_xz = Vec2::new(r(i, 2), r(i, 3)) * extent;
        if i % 2 == 1 {
            // Vertex to vertex: along grid lines, diagonals and through
            // corners, where triangles meet.
            a_xz = (a_xz / 2.0).round() * 2.0;
            b_xz = (b_xz / 2.0).round() * 2.0;
        }
        let over = t.height_at(a_xz).expect("on the map") + 0.05 + r(i, 4) * 30.0;
        let under = t.height_at(b_xz).expect("on the map") - 0.05 - r(i, 5) * 30.0;
        let a = Vec3::new(a_xz.x, over, a_xz.y);
        let b = Vec3::new(b_xz.x, under, b_xz.y);
        let length = a.distance(b);
        let dir = (b - a) / length;
        let hit = t
            .cast_ray(a, dir, length, Holes::Open)
            .unwrap_or_else(|| panic!("ray {i} from {a} to {b} slipped through"));
        let surface = t.height_at(hit.point.xz()).expect("on the map");
        assert!(
            (hit.point.y - surface).abs() < 5e-3,
            "ray {i}: off the surface"
        );
        for step in 0..(hit.distance / 0.05) as u32 {
            let p = a + dir * (step as f32 * 0.05);
            if let Some(ground) = t.height_at(p.xz()) {
                assert!(p.y - ground > -5e-3, "ray {i} was underground at {p}");
            }
        }
    }
}

#[test]
fn a_ray_from_kilometres_away_still_lands() {
    let size = GridSize::new(8, 8).expect("valid size");
    let t = Terrain::flat(size, 2.0, 0.0, MaterialId(0));
    // The ray a per-triangle test let through: 5 km long, landing mid-map.
    let origin = Vec3::new(1248.1063, 3988.2168, -2419.118);
    let dir = Vec3::new(-0.235_006_94, -0.785_561_7, 0.572_419_94);
    let hit = t
        .cast_ray(origin, dir, 50_000.0, Holes::Solid)
        .expect("lands on the map");
    assert!((hit.distance - 5076.898).abs() < 0.05, "{}", hit.distance);
    assert!(hit.point.y.abs() < 0.01);

    // Long rays aimed at vertices and edge midpoints, where triangles meet.
    let r = |i: i32, k: i32| hash01(i, k, 13);
    for i in 0..20_000 {
        let target = Vec2::new(
            (r(i, 0) * 510.0).round() + 1.0,
            (r(i, 1) * 510.0).round() + 1.0,
        );
        let from = Vec3::new(
            (r(i, 2) - 0.5) * 8000.0,
            200.0 + r(i, 3) * 4000.0,
            (r(i, 4) - 0.5) * 8000.0,
        );
        let aim = Vec3::new(target.x, 0.0, target.y) - from;
        let hit = t
            .cast_ray(from, aim, 50_000.0, Holes::Open)
            .unwrap_or_else(|| panic!("ray {i} from {from} to {target} slipped through"));
        assert!(hit.point.xz().distance(target) < 0.1, "ray {i}");
        assert!(hit.point.y.abs() < 0.02, "ray {i}");
    }
}

#[test]
fn a_ray_cast_from_a_point_on_the_ground_hits_at_once() {
    let t = hills();
    let extent = t.extent();
    for i in 0..5000 {
        let xz = Vec2::new(hash01(i, 0, 21), hash01(i, 1, 21)) * extent;
        let Some(feet) = t.point_at(xz) else {
            continue;
        };
        let hit = t
            .cast_ray(feet.position, Vec3::NEG_Y, 10.0, Holes::Open)
            .unwrap_or_else(|| panic!("fell through at {xz}"));
        assert!(hit.distance < 1e-3, "{xz}: {}", hit.distance);
        // Heading up and away from it is no hit.
        assert!(
            t.cast_ray(feet.position, Vec3::Y, 10.0, Holes::Open)
                .is_none()
        );
    }
}

#[test]
fn a_ray_under_the_ground_does_not_hit_it_from_inside() {
    // Rough ground, and a ray that starts below it and stays below.
    let size = GridSize::new(3, 2).expect("valid size");
    let mut t = Terrain::flat(size, 2.0, 0.0, MaterialId(0));
    for (x, z) in size.full_rect().iter() {
        t.set_height(x, z, 6.0 * hash01(x as i32, z as i32, 11));
    }
    let origin = Vec3::new(97.40694, 2.466_770_4, 14.493599);
    let dir = Vec3::new(0.988_596_2, -0.150_589_69, 0.000_632_012_4);
    assert!(t.height_at(origin.xz()).expect("on the map") > origin.y);
    assert!(t.cast_ray(origin, dir, 10.0, Holes::Open).is_none());
}

#[test]
fn a_ray_straight_down_cannot_miss_a_needle() {
    let size = GridSize::new(1, 1).expect("valid size");
    let mut t = Terrain::flat(size, 0.5, -1000.0, MaterialId(0));
    t.set_height(10, 10, 1000.0);
    let xz = Vec2::new(5.1, 5.2);
    let height = t.height_at(xz).expect("on the map");
    let hit = t
        .cast_ray(Vec3::new(xz.x, 5000.0, xz.y), Vec3::NEG_Y, 1e5, Holes::Open)
        .expect("the needle's flank");
    assert!(
        (hit.point.y - height).abs() < 0.01,
        "{} vs {height}",
        hit.point.y
    );
}

#[test]
fn absurd_rays_give_finite_answers() {
    let t = flat(0.0);
    let forever = f32::INFINITY;
    // Endless, straight down from a sensible height: lands.
    let hit = t
        .cast_ray(
            Vec3::new(7.0, 500.0, 7.0),
            Vec3::NEG_Y,
            forever,
            Holes::Open,
        )
        .expect("lands");
    assert!((hit.distance - 500.0).abs() < 1e-3);
    for origin in [
        Vec3::new(7.0, 1e38, 7.0),
        Vec3::new(7.0, -1e38, 7.0),
        Vec3::new(1e38, 5.0, 7.0),
        Vec3::new(f32::NAN, 5.0, 7.0),
        Vec3::new(7.0, f32::INFINITY, 7.0),
    ] {
        for dir in [Vec3::NEG_Y, Vec3::Y, Vec3::new(1.0, -1e-9, 0.0), Vec3::ZERO] {
            for max in [forever, -1.0, f32::NAN, 1e38] {
                if let Some(hit) = t.cast_ray(origin, dir, max, Holes::Open) {
                    assert!(hit.distance.is_finite() && hit.point.is_finite());
                    assert!(hit.normal.is_finite(), "{origin} {dir} {max}");
                }
            }
        }
    }
}

#[test]
fn a_hole_is_not_the_same_as_no_data() {
    let mut t = flat(0.0);
    t.set_cell(5, 10, CellFlags::default().with_hole(true));
    assert_eq!(t.ground_at(Vec2::new(11.0, 21.0)), GroundAt::Hole);
    assert_eq!(t.ground_at(Vec2::new(-1.0, 21.0)), GroundAt::Unknown);
    assert!(matches!(
        t.ground_at(Vec2::new(13.0, 21.0)),
        GroundAt::Surface(_)
    ));
}

#[test]
fn a_grazing_ray_cannot_step_over_a_spike() {
    let mut t = flat(0.0);
    t.set_height(20, 20, 0.5);
    // 20 cm up, nearly level, passing over the raised vertex at (40, 40).
    let origin = Vec3::new(2.0, 0.3, 40.0);
    let hit = t
        .cast_ray(origin, Vec3::new(1.0, -0.001, 0.0), 500.0, Holes::Open)
        .expect("the spike is in the way");
    assert!(hit.point.x > 38.0 && hit.point.x < 40.0, "{}", hit.point);
    assert!(hit.point.y > 0.2);
}

#[test]
fn straight_down_anywhere_on_the_map_lands_on_the_surface() {
    let t = hills();
    let extent = t.extent();
    // Cell interiors, grid lines, vertices and the far edges.
    let spots = [
        Vec2::new(13.37, 71.9),
        Vec2::new(64.0, 31.3),
        Vec2::new(64.0, 64.0),
        Vec2::new(0.0, 50.0),
        Vec2::new(extent.x, 50.0),
        extent,
        Vec2::new(31.0, extent.y),
    ];
    for xz in spots {
        let origin = Vec3::new(xz.x, 200.0, xz.y);
        let hit = t
            .cast_ray(origin, Vec3::NEG_Y, 1000.0, Holes::Solid)
            .unwrap_or_else(|| panic!("nothing under {xz}"));
        let height = t.height_at(xz).expect("on the map");
        assert!((hit.point.y - height).abs() < 1e-3, "{xz}");
    }
    let outside = Vec3::new(-0.1, 200.0, 50.0);
    assert!(
        t.cast_ray(outside, Vec3::NEG_Y, 1000.0, Holes::Solid)
            .is_none()
    );
}

#[test]
fn rays_from_below_and_rays_that_stop_short_miss() {
    let t = flat(5.0);
    let under = Vec3::new(10.0, 0.0, 10.0);
    assert!(t.cast_ray(under, Vec3::Y, 100.0, Holes::Open).is_none());
    let above = Vec3::new(10.0, 50.0, 10.0);
    assert!(t.cast_ray(above, Vec3::NEG_Y, 44.0, Holes::Open).is_none());
    let hit = t
        .cast_ray(above, Vec3::NEG_Y, 46.0, Holes::Open)
        .expect("in reach");
    assert!((hit.distance - 45.0).abs() < 1e-4);
    assert_eq!(hit.normal, Vec3::Y);
    assert_eq!(hit.cell, (5, 5));
    assert!(t.cast_ray(above, Vec3::ZERO, 100.0, Holes::Open).is_none());
}

#[test]
fn holes_are_open_to_characters_and_solid_to_the_cursor() {
    let mut t = flat(0.0);
    t.set_cell(5, 10, CellFlags::default().with_hole(true));
    let above = Vec3::new(11.0, 50.0, 21.0);
    assert!(t.cast_ray(above, Vec3::NEG_Y, 100.0, Holes::Open).is_none());
    assert!(
        t.cast_ray(above, Vec3::NEG_Y, 100.0, Holes::Solid)
            .is_some()
    );
    assert!(t.point_at(above.xz()).is_none());
    assert!(t.point_at(Vec2::new(13.0, 21.0)).is_some());
}

#[test]
fn a_point_is_on_the_surface_the_mesher_draws() {
    let t = hills();
    let extent = t.extent();
    for i in 0..3000 {
        let xz = Vec2::new(hash01(i, 0, 5), hash01(i, 1, 5)) * extent;
        let Some(point) = t.point_at(xz) else {
            assert!(t.is_hole_at(xz), "no ground at {xz}");
            continue;
        };
        assert_eq!(point.position.y, t.height_at(xz).expect("on the map"));
        // The normal of whichever mesh triangle holds the point.
        let (cx, cz) = t.cell_at(xz).expect("on the map");
        let ray = t
            .cast_ray(
                Vec3::new(xz.x, 500.0, xz.y),
                Vec3::NEG_Y,
                1000.0,
                Holes::Open,
            )
            .expect("ground below");
        let on_an_edge = t.cell_triangles(cx, cz).iter().any(|tri| {
            let [a, b, c] = tri.map(|(x, z)| t.vertex_position(x, z).xz());
            [(a, b), (b, c), (c, a)]
                .iter()
                .any(|(p, q)| (*q - *p).perp_dot(xz - *p).abs() < 1e-3)
        });
        if !on_an_edge {
            assert!(point.normal.dot(ray.normal) > 0.9999, "{xz}");
        }
        assert!(point.normal.y > 0.0);
    }
}

#[test]
fn steep_or_flagged_ground_is_not_walkable() {
    let mut t = flat(0.0);
    // A 3 m step across one 2 m cell: about 56°.
    for z in 0..=64 {
        for x in 11..=64 {
            t.set_height(x, z, 3.0);
        }
    }
    t.set_cell(
        30,
        30,
        CellFlags::default()
            .with_unwalkable(true)
            .with_no_clutter(true),
    );
    let at = |x: f32, z: f32| t.point_at(Vec2::new(x, z)).expect("ground");
    assert!(at(5.0, 9.0).walkable, "the flat below");
    assert!(!at(21.0, 9.0).walkable, "the step");
    assert!(at(41.0, 9.0).walkable, "the flat above");
    let flagged = at(61.0, 61.0);
    assert!(!flagged.walkable && flagged.no_clutter);
    assert!(!at(41.0, 9.0).no_clutter);
}

#[test]
fn a_chunks_collision_is_its_triangles_without_the_holes() {
    let t = hills();
    let chunk = ChunkCoord { x: 0, z: 0 };
    let mesh = t.chunk_collision(chunk).expect("on the map");
    assert_eq!(mesh.positions.len(), 33 * 33);
    // Three cells of this chunk are holes.
    assert_eq!(
        mesh.indices.len(),
        ((CHUNK_CELLS * CHUNK_CELLS - 3) * 6) as usize
    );
    let bounds = t.chunk_bounds(chunk).expect("on the map");
    for tri in mesh.indices.as_chunks::<3>().0 {
        let [a, b, c] = tri.map(|i| Vec3::from(mesh.positions[i as usize]));
        assert!((b - a).cross(c - a).y > 0.0, "faces up");
        assert!(bounds.contains(a) && bounds.contains(b) && bounds.contains(c));
    }
    assert_eq!(bounds.min.xz(), Vec2::ZERO);
    assert_eq!(bounds.max.xz(), Vec2::splat(64.0));
    assert!(bounds.max.y > bounds.min.y);

    let off = ChunkCoord { x: 3, z: 0 };
    assert!(t.chunk_collision(off).is_none());
    assert!(t.chunk_bounds(off).is_none());
}
