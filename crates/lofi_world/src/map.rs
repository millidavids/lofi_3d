//! A whole map: terrain plus everything placed on or around it.

use bevy_math::Vec3;

use crate::file::UnknownSection;
use crate::grid::GridSize;
use crate::materials::MaterialTable;
use crate::objects::ObjectLayer;
use crate::terrain::Terrain;

#[derive(Clone, Debug, PartialEq)]
pub struct WorldMap {
    pub name: String,
    pub terrain: Terrain,
    pub materials: MaterialTable,
    pub water: Water,
    pub lighting: Lighting,
    /// Trees, rocks, buildings: what the doodad tool places.
    pub objects: ObjectLayer,
    /// Data from newer tools this build doesn't understand, written back
    /// untouched on save.
    pub unknown_sections: Vec<UnknownSection>,
}

/// One sea level for the whole map. Anything sculpted below it is underwater.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Water {
    pub enabled: bool,
    pub level: f32,
    /// sRGB colour where the water is shallow.
    pub shallow: [f32; 3],
    /// sRGB colour once it's `depth` metres deep.
    pub deep: [f32; 3],
    pub depth: f32,
}

impl Default for Water {
    fn default() -> Self {
        Self {
            enabled: true,
            level: 0.0,
            shallow: [0.24, 0.56, 0.58],
            deep: [0.08, 0.22, 0.38],
            depth: 8.0,
        }
    }
}

/// Sun, ambient, sky and fog: most of a zone's mood, as in WoW's Light.dbc.
/// Colours are sRGB. Strengths are fractions of white, so `sun_strength` 1
/// lights a surface facing the sun at its full albedo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lighting {
    /// Compass bearing the sun shines from, in degrees: 0 north (-Z), 90 east.
    pub sun_azimuth: f32,
    /// Degrees above the horizon.
    pub sun_elevation: f32,
    pub sun_color: [f32; 3],
    pub sun_strength: f32,
    /// Ambient light from above, blending to `ground_ambient` from below.
    pub sky_ambient: [f32; 3],
    pub ground_ambient: [f32; 3],
    pub ambient_strength: f32,
    pub sky_zenith: [f32; 3],
    /// Also the fog colour, so distant land fades into the sky.
    pub sky_horizon: [f32; 3],
    /// Metres from the camera where fog begins, and where it's opaque.
    pub fog_start: f32,
    pub fog_end: f32,
    /// Extra fog pooling in low ground; 0 turns it off.
    pub height_fog_density: f32,
    /// How fast height fog thins with altitude, per metre.
    pub height_fog_falloff: f32,
}

impl Default for Lighting {
    fn default() -> Self {
        Self {
            sun_azimuth: 220.0,
            sun_elevation: 40.0,
            sun_color: [1.0, 0.96, 0.88],
            sun_strength: 0.9,
            sky_ambient: [0.62, 0.70, 0.84],
            ground_ambient: [0.42, 0.36, 0.28],
            ambient_strength: 0.45,
            sky_zenith: [0.30, 0.52, 0.85],
            sky_horizon: [0.70, 0.80, 0.90],
            fog_start: 600.0,
            fog_end: 1_500.0,
            height_fog_density: 0.0,
            height_fog_falloff: 0.05,
        }
    }
}

impl Lighting {
    /// Unit vector pointing at the sun.
    #[must_use]
    pub fn sun_direction(&self) -> Vec3 {
        let azimuth = self.sun_azimuth.to_radians();
        let elevation = self.sun_elevation.clamp(-89.0, 89.0).to_radians();
        let horizontal = elevation.cos();
        Vec3::new(
            azimuth.sin() * horizontal,
            elevation.sin(),
            -azimuth.cos() * horizontal,
        )
        .normalize()
    }

    /// Clamps everything into a sane range, e.g. after loading.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        let color = |c: [f32; 3]| {
            c.map(|v| {
                if v.is_finite() {
                    v.clamp(0.0, 1.0)
                } else {
                    0.5
                }
            })
        };
        let finite = |v: f32, lo: f32, hi: f32, fallback: f32| {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                fallback
            }
        };
        self.sun_azimuth = finite(self.sun_azimuth, -360.0, 720.0, 220.0);
        self.sun_elevation = finite(self.sun_elevation, -89.0, 89.0, 40.0);
        self.sun_color = color(self.sun_color);
        self.sun_strength = finite(self.sun_strength, 0.0, 4.0, 0.9);
        self.sky_ambient = color(self.sky_ambient);
        self.ground_ambient = color(self.ground_ambient);
        self.ambient_strength = finite(self.ambient_strength, 0.0, 4.0, 0.45);
        self.sky_zenith = color(self.sky_zenith);
        self.sky_horizon = color(self.sky_horizon);
        self.fog_end = finite(self.fog_end, 10.0, 100_000.0, 1_500.0);
        self.fog_start = finite(self.fog_start, 0.0, self.fog_end, 600.0);
        self.height_fog_density = finite(self.height_fog_density, 0.0, 1.0, 0.0);
        self.height_fog_falloff = finite(self.height_fog_falloff, 0.0001, 1.0, 0.05);
        self
    }
}

impl WorldMap {
    #[must_use]
    pub fn flat(name: impl Into<String>, size: GridSize, cell_size: f32, height: f32) -> Self {
        let materials = MaterialTable::starter();
        Self {
            name: name.into(),
            terrain: Terrain::flat(size, cell_size, height, materials.base()),
            materials,
            water: Water::default(),
            lighting: Lighting::default(),
            objects: ObjectLayer::default(),
            unknown_sections: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sun_direction_points_where_the_bearing_says() {
        let light = |azimuth| Lighting {
            sun_azimuth: azimuth,
            sun_elevation: 0.0,
            ..Lighting::default()
        };
        assert!((light(0.0).sun_direction() - Vec3::NEG_Z).length() < 1e-5);
        assert!((light(90.0).sun_direction() - Vec3::X).length() < 1e-5);
    }

    #[test]
    fn sanitizing_fixes_nonsense() {
        let wild = Lighting {
            fog_start: 9e9,
            fog_end: f32::NAN,
            sun_color: [2.0, -1.0, f32::INFINITY],
            ..Lighting::default()
        }
        .sanitized();
        assert!(wild.fog_start <= wild.fog_end);
        assert_eq!(wild.sun_color, [1.0, 0.0, 0.5]);
    }
}
