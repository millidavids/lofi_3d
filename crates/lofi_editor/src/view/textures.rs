//! Placeholder material textures, generated until there's real art.
//!
//! Each pattern is a small tileable detail map centred on 0.5: the shader
//! multiplies it by the material's colour, so recolouring needs no new
//! texture. Alpha is a height used for blending between materials.

use std::f32::consts::TAU;
use std::sync::OnceLock;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};
use lofi_world::{Pattern, hash01};

/// Texels per side. Small on purpose: a 4 m tile at 128 texels is 3 cm per
/// texel, and the crisp-texel filter shows them as chunky squares up close.
pub const TEXTURE_SIZE: u32 = 128;

/// Mip levels down to 1×1.
const MIP_LEVELS: u32 = TEXTURE_SIZE.ilog2() + 1;

/// One layer per pattern, in order, with a full mip chain.
pub fn material_textures(patterns: &[Pattern]) -> Image {
    let patterns: Vec<Pattern> = if patterns.is_empty() {
        vec![Pattern::Plain]
    } else {
        patterns.to_vec()
    };
    let mut data = Vec::new();
    for &pattern in &patterns {
        data.extend_from_slice(mip_chain(pattern));
    }

    let mut image = Image::new_uninit(
        Extent3d {
            width: TEXTURE_SIZE,
            height: TEXTURE_SIZE,
            depth_or_array_layers: patterns.len() as u32,
        },
        TextureDimension::D2,
        // Linear, not sRGB: these are multipliers, not colours.
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = MIP_LEVELS;
    // A one-layer array must still be viewed as an array.
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    });
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 16,
        ..default()
    });
    image
}

/// A pattern's texels, every mip level in turn. Generated once per run: the
/// material list changes far more often than the patterns do.
fn mip_chain(pattern: Pattern) -> &'static [u8] {
    static CHAINS: [OnceLock<Vec<u8>>; Pattern::ALL.len()] =
        [const { OnceLock::new() }; Pattern::ALL.len()];
    let index = Pattern::ALL
        .iter()
        .position(|&p| p == pattern)
        .unwrap_or_default();
    CHAINS[index].get_or_init(|| {
        let mut data = Vec::new();
        let mut level = generate(pattern);
        let mut size = TEXTURE_SIZE;
        loop {
            data.extend_from_slice(level.as_flattened());
            if size == 1 {
                break data;
            }
            level = downsample(&level, size);
            size /= 2;
        }
    })
}

fn generate(pattern: Pattern) -> Vec<[u8; 4]> {
    let n = TEXTURE_SIZE;
    let seed = pattern as u32 * 7919 + 13;
    let mut out = Vec::with_capacity((n * n) as usize);
    for y in 0..n {
        for x in 0..n {
            let u = x as f32 / n as f32;
            let v = y as f32 / n as f32;
            let (rgb, height) = texel(pattern, u, v, seed);
            out.push([
                to_byte(rgb[0]),
                to_byte(rgb[1]),
                to_byte(rgb[2]),
                to_byte(height),
            ]);
        }
    }
    out
}

/// Detail around 0.5, and height, for one texel at `u, v` in `0..1`.
fn texel(pattern: Pattern, u: f32, v: f32, seed: u32) -> ([f32; 3], f32) {
    let fbm = |period, octaves| periodic_fbm(u, v, period, octaves, seed);
    let noise =
        |period, salt| periodic_noise(u * period as f32, v * period as f32, period, seed + salt);
    match pattern {
        Pattern::Grass => {
            let patches = fbm(4, 3);
            let blades = noise(64, 1);
            let lum = 0.5 + (patches - 0.5) * 0.22 + (blades - 0.5) * 0.24;
            // Blades lean a touch yellow, patches a touch blue.
            let warm = (blades - 0.5) * 0.06;
            (
                [lum + warm, lum + warm * 0.5, lum - warm],
                blades * 0.6 + patches * 0.4,
            )
        }
        Pattern::Dirt => {
            let base = fbm(4, 4);
            let pebble = smooth(0.74, 0.82, noise(32, 2));
            let lum = 0.5 + (base - 0.5) * 0.28 + pebble * 0.12;
            ([lum; 3], base * 0.5 + pebble * 0.5)
        }
        Pattern::Rock => {
            let mass = fbm(2, 4);
            let ridge = 1.0 - (noise(8, 3) * 2.0 - 1.0).abs();
            let crack = smooth(0.88, 0.97, ridge);
            let lum = 0.5 + (mass - 0.5) * 0.32 - crack * 0.22;
            ([lum; 3], mass * (1.0 - crack))
        }
        Pattern::Sand => {
            let grain = noise(64, 4);
            let drift = fbm(2, 2);
            let ripple = (((v * 10.0) + drift * 2.0) * TAU).sin() * 0.5 + 0.5;
            let lum = 0.5 + (grain - 0.5) * 0.10 + (ripple - 0.5) * 0.08;
            ([lum; 3], ripple * 0.3 + grain * 0.2)
        }
        Pattern::Snow => {
            let drift = fbm(4, 3);
            let sparkle = smooth(0.9, 0.97, noise(64, 5)) * 0.08;
            let lum = 0.5 + (drift - 0.5) * 0.06 + sparkle;
            ([lum, lum, lum + 0.01], drift)
        }
        Pattern::Cobble => {
            let (near, gap, cell) = periodic_cells(u, v, 6, seed);
            let mortar = 1.0 - smooth(0.02, 0.06, gap);
            let stone = 0.5 + (hash01(cell.0, cell.1, seed) - 0.5) * 0.2 - near * 0.15;
            let lum = stone * (1.0 - mortar) + 0.3 * mortar;
            ([lum; 3], (0.8 - near) * (1.0 - mortar))
        }
        Pattern::Plain => {
            let lum = 0.5 + (fbm(8, 3) - 0.5) * 0.06;
            ([lum; 3], 0.5)
        }
    }
}

/// Value noise on a lattice that wraps every `period` cells, so it tiles.
fn periodic_noise(x: f32, z: f32, period: u32, seed: u32) -> f32 {
    let period = period as i32;
    let (x0, z0) = (x.floor() as i32, z.floor() as i32);
    let (fx, fz) = (x - x0 as f32, z - z0 as f32);
    let (sx, sz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let corner = |dx: i32, dz: i32| {
        hash01(
            (x0 + dx).rem_euclid(period),
            (z0 + dz).rem_euclid(period),
            seed,
        )
    };
    let bottom = corner(0, 0) + (corner(1, 0) - corner(0, 0)) * sx;
    let top = corner(0, 1) + (corner(1, 1) - corner(0, 1)) * sx;
    bottom + (top - bottom) * sz
}

/// Tileable fractal noise in 0..1, starting at `period` cells per tile.
fn periodic_fbm(u: f32, v: f32, period: u32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut norm, mut amplitude) = (0.0, 0.0, 1.0);
    for octave in 0..octaves {
        let p = period << octave;
        sum += amplitude * periodic_noise(u * p as f32, v * p as f32, p, seed + octave * 31);
        norm += amplitude;
        amplitude *= 0.5;
    }
    sum / norm
}

/// Tileable Voronoi: distance to the nearest cell centre, the gap to the
/// second nearest, and the nearest cell's lattice coordinates.
fn periodic_cells(u: f32, v: f32, cells: u32, seed: u32) -> (f32, f32, (i32, i32)) {
    let n = cells as i32;
    let (x, z) = (u * cells as f32, v * cells as f32);
    let (cx, cz) = (x.floor() as i32, z.floor() as i32);
    let mut best = (f32::MAX, f32::MAX, (0, 0));
    for dz in -1..=1 {
        for dx in -1..=1 {
            let (gx, gz) = (cx + dx, cz + dz);
            let wrapped = (gx.rem_euclid(n), gz.rem_euclid(n));
            let px = gx as f32 + hash01(wrapped.0, wrapped.1, seed + 1);
            let pz = gz as f32 + hash01(wrapped.0, wrapped.1, seed + 2);
            let d = ((px - x).powi(2) + (pz - z).powi(2)).sqrt();
            if d < best.0 {
                best = (d, best.0, wrapped);
            } else if d < best.1 {
                best.1 = d;
            }
        }
    }
    (best.0, best.1 - best.0, best.2)
}

fn downsample(level: &[[u8; 4]], size: u32) -> Vec<[u8; 4]> {
    let half = size / 2;
    let at = |x: u32, y: u32| level[(y * size + x) as usize];
    (0..half)
        .flat_map(|y| (0..half).map(move |x| (x, y)))
        .map(|(x, y)| {
            let texels = [
                at(2 * x, 2 * y),
                at(2 * x + 1, 2 * y),
                at(2 * x, 2 * y + 1),
                at(2 * x + 1, 2 * y + 1),
            ];
            std::array::from_fn::<u8, 4, _>(|c| {
                let sum: u32 = texels.iter().map(|t| u32::from(t[c])).sum();
                ((sum + 2) / 4) as u8
            })
        })
        .collect()
}

fn smooth(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn to_byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pattern_tiles_and_centres_on_half() {
        for pattern in Pattern::ALL {
            let texels = generate(pattern);
            let n = TEXTURE_SIZE as usize;
            // Opposite edges should be close, as neighbours across the seam.
            let seam: f32 = (0..n)
                .map(|y| f32::from(texels[y * n][0]) - f32::from(texels[y * n + n - 1][0]))
                .map(f32::abs)
                .sum::<f32>()
                / n as f32;
            assert!(seam < 20.0, "{pattern:?} seam {seam}");
            let mean: f32 =
                texels.iter().map(|t| f32::from(t[1])).sum::<f32>() / texels.len() as f32;
            assert!((100.0..160.0).contains(&mean), "{pattern:?} mean {mean}");
        }
    }

    /// Writes each pattern as a PNG, tiled 2×2 so seams show, for eyeballing.
    /// `LOFI_TEXTURE_DUMP=/some/dir cargo test -p lofi_terrain dump_patterns -- --ignored`
    #[test]
    #[ignore = "writes images; for looking at the patterns"]
    fn dump_patterns() {
        let dir = std::env::var("LOFI_TEXTURE_DUMP").expect("set LOFI_TEXTURE_DUMP");
        let n = TEXTURE_SIZE;
        for pattern in Pattern::ALL {
            let texels = generate(pattern);
            let tiled: Vec<u8> = (0..n * 2)
                .flat_map(|y| (0..n * 2).map(move |x| (x % n, y % n)))
                .flat_map(|(x, y)| {
                    let [r, g, b, _] = texels[(y * n + x) as usize];
                    [r, g, b, 255]
                })
                .collect();
            let image = Image::new(
                Extent3d {
                    width: n * 2,
                    height: n * 2,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                tiled,
                // Only sRGB converts; the bytes are written as-is either way.
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::MAIN_WORLD,
            );
            let path = format!("{dir}/{}.png", pattern.label());
            image
                .try_into_dynamic()
                .expect("rgba8")
                .save(&path)
                .expect("saves");
        }
    }

    #[test]
    fn mip_chain_is_complete() {
        let image = material_textures(&[Pattern::Grass, Pattern::Rock]);
        let per_layer: usize = (0..8).map(|l| (128usize >> l).pow(2) * 4).sum();
        assert_eq!(image.texture_descriptor.mip_level_count, 8);
        assert_eq!(image.data.as_ref().map(Vec::len), Some(per_layer * 2));
    }
}
