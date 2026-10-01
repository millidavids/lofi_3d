//! Octahedral encoding: a unit vector in two signed bytes, error under 1°.

use bevy_math::Vec3;

pub(super) fn encode_normal(n: Vec3) -> [i8; 2] {
    let n = n / (n.x.abs() + n.y.abs() + n.z.abs());
    // Y is up, and terrain normals always point up, so fold on y.
    let (a, b) = if n.y >= 0.0 {
        (n.x, n.z)
    } else {
        (
            (1.0 - n.z.abs()) * n.x.signum(),
            (1.0 - n.x.abs()) * n.z.signum(),
        )
    };
    [a, b].map(|v| (v * 127.0).round().clamp(-127.0, 127.0) as i8)
}

pub(super) fn decode_normal([a, b]: [i8; 2]) -> Vec3 {
    let (x, z) = (f32::from(a) / 127.0, f32::from(b) / 127.0);
    let y = 1.0 - x.abs() - z.abs();
    let (x, z) = if y < 0.0 {
        ((1.0 - z.abs()) * x.signum(), (1.0 - x.abs()) * z.signum())
    } else {
        (x, z)
    };
    Vec3::new(x, y, z).normalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normals_survive_encoding() {
        for n in [
            Vec3::Y,
            Vec3::new(0.3, 0.9, -0.2),
            Vec3::new(-0.7, 0.2, 0.6),
            Vec3::new(0.0, -1.0, 0.0),
        ] {
            let n = n.normalize();
            let back = decode_normal(encode_normal(n));
            assert!(
                back.angle_between(n) < 1.5_f32.to_radians(),
                "{n} -> {back}"
            );
        }
    }
}
