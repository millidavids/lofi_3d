//! Which object the pointer is on: the nearest whose bounding sphere the ray
//! enters before it reaches the ground.

use bevy::prelude::*;
use lofi_world::{ObjectId, ObjectLayer, asset};

/// Spheres poke into the ground they stand on, so an object counts even if
/// the ray meets the ground this far in front of it.
const GROUND_SLACK: f32 = 1.0;

/// `ground` is how far along the ray the ground is, if it's hit at all.
pub fn pick_object(objects: &ObjectLayer, ray: Ray3d, ground: Option<f32>) -> Option<ObjectId> {
    let dir = *ray.direction;
    let mut best: Option<(f32, ObjectId)> = None;
    for o in objects.iter() {
        let (up, radius) = asset(o.asset).bounds(o.scale);
        let centre = o.position + Vec3::Y * up;
        let oc = ray.origin - centre;
        let b = oc.dot(dir);
        let c = oc.length_squared() - radius * radius;
        let disc = b * b - c;
        if disc < 0.0 {
            continue;
        }
        let root = disc.sqrt();
        // Entering, or already inside.
        let t = if -b - root >= 0.0 {
            -b - root
        } else {
            -b + root
        };
        if t < 0.0 || ground.is_some_and(|g| t > g + GROUND_SLACK) {
            continue;
        }
        if best.is_none_or(|(nearest, _)| t < nearest) {
            best = Some((t, o.id));
        }
    }
    best.map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    use lofi_world::BUILTIN_ASSETS;

    use super::*;

    fn ray(origin: Vec3, toward: Vec3) -> Ray3d {
        Ray3d::new(origin, Dir3::new(toward - origin).expect("non-zero"))
    }

    #[test]
    fn nearest_wins_and_the_ground_hides_what_is_behind_it() {
        let mut layer = ObjectLayer::default();
        let pine = BUILTIN_ASSETS[1].guid;
        let near = layer
            .add(pine, Vec3::new(0.0, 0.0, 10.0), 0.0, 1.0)
            .expect("room");
        let far = layer
            .add(pine, Vec3::new(0.0, 0.0, 30.0), 0.0, 1.0)
            .expect("room");
        let look = ray(Vec3::new(0.0, 4.0, 0.0), Vec3::new(0.0, 4.0, 40.0));
        assert_eq!(pick_object(&layer, look, None), Some(near));
        // A hill at 20 m: the far one is behind it.
        layer.remove(near);
        assert_eq!(pick_object(&layer, look, Some(20.0)), None);
        assert_eq!(pick_object(&layer, look, None), Some(far));
    }

    #[test]
    fn the_top_of_a_tall_shape_can_be_picked() {
        let mut layer = ObjectLayer::default();
        let pine = BUILTIN_ASSETS[1].guid;
        let id = layer.add(pine, Vec3::ZERO, 0.0, 1.0).expect("room");
        // Straight down onto the tip, 7.5 m up.
        let down = ray(Vec3::new(0.3, 50.0, 0.0), Vec3::new(0.3, 0.0, 0.0));
        assert_eq!(pick_object(&layer, down, Some(50.0)), Some(id));
    }
}
