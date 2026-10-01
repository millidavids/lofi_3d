//! Low-poly placeholder meshes for the built-in assets: flat-shaded, colour
//! baked into the vertices, base at the origin, up +Y.

use std::f32::consts::TAU;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use lofi_world::{Asset, Shape, hash01};

const TRUNK: [f32; 3] = [0.34, 0.24, 0.15];
const ROOF: [f32; 3] = [0.45, 0.22, 0.15];

/// A mesh for `asset`, in its own colours.
pub fn asset_mesh(asset: &Asset) -> Mesh {
    let mut b = Builder::default();
    let main = asset.color;
    match asset.shape {
        Shape::Tree => {
            b.prism(Vec3::ZERO, 0.3, 2.6, 6, TRUNK);
            b.ball(
                Vec3::new(0.0, 4.0, 0.0),
                Vec3::new(2.3, 2.0, 2.3),
                5,
                8,
                main,
                0.0,
            );
        }
        Shape::Pine => {
            b.prism(Vec3::ZERO, 0.25, 1.4, 6, TRUNK);
            for (y, r, h) in [(1.0, 2.0, 3.0), (2.8, 1.6, 2.8), (4.6, 1.1, 3.4)] {
                b.cone(Vec3::new(0.0, y, 0.0), r, h, 8, main);
            }
        }
        Shape::Rock => {
            b.ball(
                Vec3::new(0.0, 0.6, 0.0),
                Vec3::new(1.5, 0.9, 1.3),
                4,
                7,
                main,
                0.12,
            );
        }
        Shape::Crate => b.cuboid(Vec3::new(-0.6, 0.0, -0.6), Vec3::new(0.6, 1.2, 0.6), main),
        Shape::Hut => {
            b.cuboid(Vec3::new(-2.0, 0.0, -2.0), Vec3::new(2.0, 2.4, 2.0), main);
            b.roof(2.4, 4.2, 2.3, ROOF);
        }
        Shape::Pillar => {
            b.cuboid(Vec3::new(-0.6, 0.0, -0.6), Vec3::new(0.6, 0.4, 0.6), main);
            b.prism(Vec3::new(0.0, 0.4, 0.0), 0.4, 4.2, 8, main);
            b.cuboid(Vec3::new(-0.6, 4.6, -0.6), Vec3::new(0.6, 5.0, 0.6), main);
        }
        Shape::Unknown => b.cuboid(Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 1.0, 0.5), main),
    }
    b.build()
}

/// Separate vertices per triangle, so every face is flat.
#[derive(Default)]
struct Builder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
}

impl Builder {
    /// Counter-clockwise seen from outside.
    fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, srgb: [f32; 3]) {
        let normal = (b - a).cross(c - a).normalize_or_zero();
        let color = Color::srgb(srgb[0], srgb[1], srgb[2])
            .to_linear()
            .to_f32_array();
        for p in [a, b, c] {
            self.positions.push(p.to_array());
            self.normals.push(normal.to_array());
            self.colors.push(color);
        }
    }

    fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, srgb: [f32; 3]) {
        self.tri(a, b, c, srgb);
        self.tri(a, c, d, srgb);
    }

    fn ring(centre: Vec3, radius: f32, sides: u32) -> Vec<Vec3> {
        (0..sides)
            .map(|i| {
                let a = TAU * i as f32 / sides as f32;
                centre + Vec3::new(a.cos(), 0.0, -a.sin()) * radius
            })
            .collect()
    }

    /// An upright n-sided column with caps.
    fn prism(&mut self, base: Vec3, radius: f32, height: f32, sides: u32, srgb: [f32; 3]) {
        let top = base + Vec3::Y * height;
        let low = Self::ring(base, radius, sides);
        let high = Self::ring(top, radius, sides);
        for i in 0..sides as usize {
            let j = (i + 1) % sides as usize;
            self.quad(low[i], low[j], high[j], high[i], srgb);
            self.tri(top, high[i], high[j], srgb);
            self.tri(base, low[j], low[i], srgb);
        }
    }

    fn cone(&mut self, base: Vec3, radius: f32, height: f32, sides: u32, srgb: [f32; 3]) {
        let tip = base + Vec3::Y * height;
        let ring = Self::ring(base, radius, sides);
        for i in 0..sides as usize {
            let j = (i + 1) % sides as usize;
            self.tri(ring[i], ring[j], tip, srgb);
            self.tri(base, ring[j], ring[i], srgb);
        }
    }

    /// A faceted ellipsoid; `jitter` roughens it repeatably, for rocks.
    fn ball(
        &mut self,
        centre: Vec3,
        radii: Vec3,
        rings: u32,
        sectors: u32,
        srgb: [f32; 3],
        jitter: f32,
    ) {
        let point = |ring: u32, sector: u32| {
            let polar = std::f32::consts::PI * ring as f32 / rings as f32;
            let azimuth = TAU * (sector % sectors) as f32 / sectors as f32;
            let unit = Vec3::new(
                polar.sin() * azimuth.cos(),
                polar.cos(),
                -polar.sin() * azimuth.sin(),
            );
            // The poles and the seam must agree, so jitter by where, not when.
            let wobble = if ring == 0 || ring == rings {
                1.0
            } else {
                1.0 + (hash01(ring as i32, (sector % sectors) as i32, 7) - 0.5) * 2.0 * jitter
            };
            centre + unit * radii * wobble
        };
        for ring in 0..rings {
            for sector in 0..sectors {
                let a = point(ring, sector);
                let b = point(ring + 1, sector);
                let c = point(ring + 1, sector + 1);
                let d = point(ring, sector + 1);
                if ring > 0 {
                    self.tri(a, b, d, srgb);
                }
                if ring + 1 < rings {
                    self.tri(d, b, c, srgb);
                }
            }
        }
    }

    fn cuboid(&mut self, min: Vec3, max: Vec3, srgb: [f32; 3]) {
        let p = |x: bool, y: bool, z: bool| {
            Vec3::new(
                if x { max.x } else { min.x },
                if y { max.y } else { min.y },
                if z { max.z } else { min.z },
            )
        };
        let (f, t) = (false, true);
        self.quad(p(f, f, t), p(t, f, t), p(t, t, t), p(f, t, t), srgb); // +z
        self.quad(p(t, f, f), p(f, f, f), p(f, t, f), p(t, t, f), srgb); // -z
        self.quad(p(t, f, t), p(t, f, f), p(t, t, f), p(t, t, t), srgb); // +x
        self.quad(p(f, f, f), p(f, f, t), p(f, t, t), p(f, t, f), srgb); // -x
        self.quad(p(f, t, t), p(t, t, t), p(t, t, f), p(f, t, f), srgb); // +y
        self.quad(p(f, f, f), p(t, f, f), p(t, f, t), p(f, f, t), srgb); // -y
    }

    /// A pitched roof along X, eaves at `from`, ridge at `to`, over a square
    /// of half-width `half`.
    fn roof(&mut self, from: f32, to: f32, half: f32, srgb: [f32; 3]) {
        let eave = |x: f32, z: f32| Vec3::new(x, from, z);
        let ridge = |x: f32| Vec3::new(x, to, 0.0);
        let (w, e) = (-half, half);
        self.quad(eave(w, half), eave(e, half), ridge(e), ridge(w), srgb);
        self.quad(eave(e, -half), eave(w, -half), ridge(w), ridge(e), srgb);
        self.tri(eave(e, half), eave(e, -half), ridge(e), srgb);
        self.tri(eave(w, -half), eave(w, half), ridge(w), srgb);
        self.quad(
            eave(w, -half),
            eave(e, -half),
            eave(e, half),
            eave(w, half),
            srgb,
        );
    }

    fn build(self) -> Mesh {
        let count = self.positions.len() as u32;
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
        .with_inserted_indices(Indices::U32((0..count).collect()))
    }
}

#[cfg(test)]
mod tests {
    use bevy::mesh::VertexAttributeValues;
    use lofi_world::{BUILTIN_ASSETS, UNKNOWN_ASSET};

    use super::*;

    #[test]
    fn every_shape_faces_out_and_fits_its_bounds() {
        for asset in BUILTIN_ASSETS.iter().chain([&UNKNOWN_ASSET]) {
            let mesh = asset_mesh(asset);
            let Some(VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("positions");
            };
            let Some(VertexAttributeValues::Float32x3(normals)) =
                mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                panic!("normals");
            };
            let (centre_up, radius) = asset.bounds(1.0);
            let centre = Vec3::Y * centre_up;
            for (p, n) in positions.iter().zip(normals) {
                let p = Vec3::from(*p);
                assert!(
                    p.distance(centre) <= radius + 1e-3,
                    "{}: {p} outside its bounds",
                    asset.name
                );
                assert!(
                    Vec3::from(*n).length() > 0.5,
                    "{}: a degenerate face",
                    asset.name
                );
            }
            // Faces point away from the shape's middle, roughly.
            let mut outward = 0;
            for (tri, n) in positions.chunks(3).zip(normals.chunks(3)) {
                let middle = (Vec3::from(tri[0]) + Vec3::from(tri[1]) + Vec3::from(tri[2])) / 3.0;
                if (middle - centre).dot(Vec3::from(n[0])) >= -1e-3 {
                    outward += 1;
                }
            }
            let faces = positions.len() / 3;
            assert!(
                outward * 10 >= faces * 8,
                "{}: {outward}/{faces} face out",
                asset.name
            );
        }
    }
}
