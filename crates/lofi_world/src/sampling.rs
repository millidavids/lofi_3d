//! Reading the surface between and around vertices.

use bevy_math::{Vec2, Vec3};

use crate::grid::VertexRect;
use crate::ground::{Ground, cell_plane, locate};
use crate::terrain::Terrain;

impl Terrain {
    /// Height of the surface at a world XZ, or `None` off the map.
    ///
    /// Interpolates on the same triangles [`crate::build_chunk_mesh`] emits,
    /// so the brush cursor sits exactly on what you see. Holes still have a
    /// height here, the one they'd have filled in; see [`Self::is_hole_at`].
    #[must_use]
    pub fn height_at(&self, xz: Vec2) -> Option<f32> {
        let (cx, cz, u, v) = locate(self.cell_size(), self.cell_counts(), xz)?;
        let corners = [
            self.height(cx, cz),
            self.height(cx + 1, cz),
            self.height(cx, cz + 1),
            self.height(cx + 1, cz + 1),
        ];
        Some(cell_plane(corners, self.splits_on_main_diagonal(cx, cz), u, v).height)
    }

    /// Smooth vertex normal from central differences over the heightmap.
    ///
    /// Reads neighbours across chunk borders, so smooth shading has no seams.
    #[must_use]
    pub fn normal(&self, x: u32, z: u32) -> Vec3 {
        let x0 = x.saturating_sub(1);
        let x1 = (x + 1).min(self.size().cells_x());
        let z0 = z.saturating_sub(1);
        let z1 = (z + 1).min(self.size().cells_z());
        let dx = (self.height(x1, z) - self.height(x0, z)) / ((x1 - x0) as f32 * self.cell_size());
        let dz = (self.height(x, z1) - self.height(x, z0)) / ((z1 - z0) as f32 * self.cell_size());
        Vec3::new(-dx, 1.0, -dz).normalize()
    }

    /// Vertices inside the square around `center`, or `None` if it misses the map.
    #[must_use]
    pub fn vertices_near(&self, center: Vec2, radius: f32) -> Option<VertexRect> {
        let lo = ((center - radius) / self.cell_size()).ceil();
        let hi = ((center + radius) / self.cell_size()).floor();
        let max = Vec2::new(self.size().cells_x() as f32, self.size().cells_z() as f32);
        if hi.x < 0.0 || hi.y < 0.0 || lo.x > max.x || lo.y > max.y {
            return None;
        }
        let lo = lo.clamp(Vec2::ZERO, max);
        let hi = hi.clamp(Vec2::ZERO, max);
        if lo.x > hi.x || lo.y > hi.y {
            return None;
        }
        Some(VertexRect {
            min_x: lo.x as u32,
            min_z: lo.y as u32,
            max_x: hi.x as u32,
            max_z: hi.y as u32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::GridSize;
    use crate::terrain::bumpy;

    #[test]
    fn height_at_matches_vertices() {
        let t = bumpy();
        for (x, z) in t.size().full_rect().iter() {
            let p = t.vertex_position(x, z);
            let h = t.height_at(Vec2::new(p.x, p.z)).expect("on map");
            assert!((h - p.y).abs() < 1e-4, "({x},{z}) {h} != {}", p.y);
        }
    }

    #[test]
    fn height_at_is_continuous_across_cell_edges() {
        let t = bumpy();
        for i in 1..60 {
            let x = i as f32 * 1.03;
            let edge = 2.0 * 7.0;
            let left = t.height_at(Vec2::new(x, edge - 1e-4)).expect("on map");
            let right = t.height_at(Vec2::new(x, edge + 1e-4)).expect("on map");
            assert!((left - right).abs() < 1e-2);
        }
    }

    /// Interpolates on whichever of the cell's mesh triangles holds `xz`.
    fn height_on_mesh(t: &Terrain, xz: Vec2) -> f32 {
        use bevy_math::Vec3Swizzles;

        let grid = xz / t.cell_size();
        let (cx, cz) = (grid.x as u32, grid.y as u32);
        for tri in t.cell_triangles(cx, cz) {
            let [p0, p1, p2] = tri.map(|(x, z)| t.vertex_position(x, z));
            let (a, b, c) = (p0.xz(), p1.xz(), p2.xz());
            let area = (b - a).perp_dot(c - a);
            let w1 = (xz - a).perp_dot(c - a) / area;
            let w2 = (b - a).perp_dot(xz - a) / area;
            let w0 = 1.0 - w1 - w2;
            if w0 >= -1e-5 && w1 >= -1e-5 && w2 >= -1e-5 {
                return w0 * p0.y + w1 * p1.y + w2 * p2.y;
            }
        }
        panic!("{xz} is in neither triangle");
    }

    #[test]
    fn height_at_matches_the_mesh_triangles_under_every_rule() {
        use crate::cells::{CellFlags, Diagonal, Triangulation};
        use crate::noise::hash01;

        let mut t = bumpy();
        // Some forced cells among the auto ones.
        t.set_cell(2, 2, CellFlags::default().with_diagonal(Diagonal::Main));
        t.set_cell(3, 2, CellFlags::default().with_diagonal(Diagonal::Anti));
        for rule in [Triangulation::Checkerboard, Triangulation::FollowTerrain] {
            t.set_triangulation(rule);
            for i in 0..2000 {
                let xz = Vec2::new(hash01(i, 0, 1), hash01(i, 0, 2)) * 63.9;
                let h = t.height_at(xz).expect("on map");
                let expected = height_on_mesh(&t, xz);
                assert!(
                    (h - expected).abs() < 1e-3,
                    "{rule:?} at {xz}: {h} vs {expected}"
                );
            }
        }
    }

    #[test]
    fn height_at_is_none_off_map() {
        let t = bumpy();
        assert!(t.height_at(Vec2::new(-0.1, 1.0)).is_none());
        assert!(t.height_at(Vec2::new(1.0, 64.1)).is_none());
        assert!(t.height_at(Vec2::new(64.0, 64.0)).is_some());
    }

    #[test]
    fn flat_normals_point_up() {
        let size = GridSize::new(1, 1).expect("valid size");
        let t = Terrain::flat(size, 2.0, 5.0, crate::materials::MaterialId(0));
        assert!((t.normal(3, 4) - Vec3::Y).length() < 1e-6);
    }

    #[test]
    fn vertices_near_clamps_and_misses() {
        let t = bumpy();
        let rect = t.vertices_near(Vec2::new(1.0, 1.0), 3.0).expect("hits map");
        assert_eq!(
            (rect.min_x, rect.min_z, rect.max_x, rect.max_z),
            (0, 0, 2, 2)
        );
        assert!(t.vertices_near(Vec2::new(-50.0, 10.0), 3.0).is_none());
    }
}
