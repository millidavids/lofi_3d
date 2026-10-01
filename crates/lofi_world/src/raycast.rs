//! Ray against the terrain, for picking what's under the mouse. Shorthand
//! for [`Ground::cast_ray`], which the game's collision uses too.

use bevy_math::Vec3;

use crate::ground::{Ground, Holes};
use crate::terrain::Terrain;

impl Terrain {
    /// First point where the ray meets the surface from above. Rays that
    /// start under the surface miss, and so do rays that drop through a hole.
    #[must_use]
    pub fn raycast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<Vec3> {
        self.cast_ray(origin, direction, max_distance, Holes::Open)
            .map(|hit| hit.point)
    }

    /// Like [`Self::raycast`], but holes stop the ray as if filled in. For an
    /// editor cursor, which must be able to point at a hole to fill it.
    #[must_use]
    pub fn raycast_solid(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<Vec3> {
        self.cast_ray(origin, direction, max_distance, Holes::Solid)
            .map(|hit| hit.point)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::GridSize;

    fn flat(height: f32) -> Terrain {
        let size = GridSize::new(2, 2).expect("valid size");
        Terrain::flat(size, 2.0, height, crate::materials::MaterialId(0))
    }

    #[test]
    fn straight_down_hits_the_plane() {
        let hit = flat(5.0)
            .raycast(Vec3::new(10.0, 50.0, 20.0), Vec3::NEG_Y, 1000.0)
            .expect("hit");
        assert!((hit - Vec3::new(10.0, 5.0, 20.0)).length() < 1e-3);
    }

    #[test]
    fn angled_ray_from_outside_the_map_hits() {
        let t = flat(0.0);
        let hit = t
            .raycast(
                Vec3::new(-20.0, 40.0, 64.0),
                Vec3::new(1.0, -1.0, 0.0),
                1000.0,
            )
            .expect("hit");
        assert!((hit - Vec3::new(20.0, 0.0, 64.0)).length() < 1e-2);
    }

    #[test]
    fn rays_fall_through_holes() {
        let mut t = flat(0.0);
        t.set_cell(5, 10, crate::cells::CellFlags::default().with_hole(true));
        // Straight down into the hole.
        assert!(
            t.raycast(Vec3::new(11.0, 50.0, 21.0), Vec3::NEG_Y, 1000.0)
                .is_none()
        );
        // Angled into the hole, then on under the solid cell beyond it.
        assert!(
            t.raycast(
                Vec3::new(10.5, 1.0, 21.0),
                Vec3::new(1.0, -1.0, 0.0),
                1000.0
            )
            .is_none()
        );
        // Over the hole and down onto solid ground past it.
        assert!(
            t.raycast(
                Vec3::new(10.5, 1.0, 21.0),
                Vec3::new(1.0, -0.2, 0.0),
                1000.0
            )
            .is_some_and(|hit| hit.x > 12.0)
        );
        // Next door is solid.
        assert!(
            t.raycast(Vec3::new(9.0, 50.0, 21.0), Vec3::NEG_Y, 1000.0)
                .is_some()
        );
        // An editor cursor can still point at the hole.
        let hit = t
            .raycast_solid(Vec3::new(11.0, 50.0, 21.0), Vec3::NEG_Y, 1000.0)
            .expect("hits the filled-in hole");
        assert!((hit - Vec3::new(11.0, 0.0, 21.0)).length() < 1e-3);
    }

    #[test]
    fn finds_a_raised_bump() {
        let mut t = flat(0.0);
        t.set_height(16, 16, 20.0);
        let hit = t
            .raycast(Vec3::new(32.0, 50.0, 32.0), Vec3::NEG_Y, 1000.0)
            .expect("hit");
        assert!((hit.y - 20.0).abs() < 1e-3);
    }

    #[test]
    fn misses_when_looking_up_or_off_map() {
        let t = flat(0.0);
        assert!(
            t.raycast(Vec3::new(10.0, 5.0, 10.0), Vec3::Y, 1000.0)
                .is_none()
        );
        assert!(
            t.raycast(Vec3::new(-10.0, 5.0, 10.0), Vec3::NEG_X, 1000.0)
                .is_none()
        );
    }

    #[test]
    fn starting_underground_misses() {
        let t = flat(10.0);
        assert!(
            t.raycast(Vec3::new(10.0, 5.0, 10.0), Vec3::NEG_Y, 1000.0)
                .is_none()
        );
    }
}
