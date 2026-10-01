//! The custom surface materials: terrain, water and sky.
//!
//! None of them use Bevy's PBR lighting. They share the light table in the
//! map and one fog function (shaders/fog.wgsl), so everything fades together.

use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use lofi_world::MAX_MATERIALS;

// `embedded_asset!` files them under the crate's name, so follow it.
const TERRAIN_SHADER: &str = concat!(
    "embedded://",
    env!("CARGO_CRATE_NAME"),
    "/view/shaders/terrain.wgsl"
);
const WATER_SHADER: &str = concat!(
    "embedded://",
    env!("CARGO_CRATE_NAME"),
    "/view/shaders/water.wgsl"
);
const SKY_SHADER: &str = concat!(
    "embedded://",
    env!("CARGO_CRATE_NAME"),
    "/view/shaders/sky.wgsl"
);
const OBJECTS_SHADER: &str = concat!(
    "embedded://",
    env!("CARGO_CRATE_NAME"),
    "/view/shaders/objects.wgsl"
);

/// Terrain shader flags.
pub const FLAG_FLAT: u32 = 1;
pub const FLAG_CRISP: u32 = 2;
pub const FLAG_SLOPE: u32 = 4;
pub const FLAG_CHUNK_GRID: u32 = 8;

#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct FogUniform {
    /// Linear RGB; alpha 0 turns fog off.
    pub color: Vec4,
    /// Start, end, height density, height falloff.
    pub distance: Vec4,
}

#[derive(ShaderType, Clone, Debug, PartialEq)]
pub struct TerrainParams {
    /// Linear RGB per material layer; w is 1 / tile size.
    pub colors: [Vec4; MAX_MATERIALS],
    pub sky_ambient: Vec4,
    pub ground_ambient: Vec4,
    pub fog: FogUniform,
    pub flags: u32,
    pub texture_size: f32,
    /// Metres per chunk side.
    pub chunk_size: f32,
    /// Cosine of the steepest walkable slope.
    pub walkable_cos: f32,
}

impl Default for TerrainParams {
    fn default() -> Self {
        Self {
            colors: [Vec4::new(0.5, 0.5, 0.5, 0.25); MAX_MATERIALS],
            sky_ambient: Vec4::splat(0.3),
            ground_ambient: Vec4::splat(0.2),
            fog: FogUniform::default(),
            flags: 0,
            texture_size: 1.0,
            chunk_size: 64.0,
            walkable_cos: 0.64,
        }
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct TerrainMaterial {
    #[uniform(0)]
    pub params: TerrainParams,
    #[texture(1, dimension = "2d_array")]
    #[sampler(2)]
    pub textures: Handle<Image>,
}

impl Material for TerrainMaterial {
    fn vertex_shader() -> ShaderRef {
        TERRAIN_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        TERRAIN_SHADER.into()
    }
}

#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct WaterParams {
    pub shallow: Vec4,
    pub deep: Vec4,
    pub ambient: Vec4,
    pub fog: FogUniform,
    pub depth_range: f32,
    /// Sea level. Meshes carry ground height, so moving it rebuilds nothing.
    pub level: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct WaterMaterial {
    #[uniform(0)]
    pub params: WaterParams,
}

impl Material for WaterMaterial {
    fn vertex_shader() -> ShaderRef {
        WATER_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        WATER_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn enable_shadows() -> bool {
        false
    }
}

#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct SkyParams {
    pub zenith: Vec4,
    pub horizon: Vec4,
    pub sun_color: Vec4,
    pub sun_direction: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct SkyMaterial {
    #[uniform(0)]
    pub params: SkyParams,
}

impl Material for SkyMaterial {
    fn vertex_shader() -> ShaderRef {
        SKY_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SKY_SHADER.into()
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    /// The camera sits inside the dome, so draw its inside faces.
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct ObjectParams {
    pub sky_ambient: Vec4,
    pub ground_ambient: Vec4,
    pub fog: FogUniform,
}

/// Every placed object shares this; their colours are in their meshes.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct ObjectMaterial {
    #[uniform(0)]
    pub params: ObjectParams,
}

impl Material for ObjectMaterial {
    fn vertex_shader() -> ShaderRef {
        OBJECTS_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        OBJECTS_SHADER.into()
    }
}

/// sRGB triple to linear `Vec4` with the given alpha.
pub fn linear(srgb: [f32; 3], alpha: f32) -> Vec4 {
    let c = Color::srgb(srgb[0], srgb[1], srgb[2]).to_linear();
    Vec4::new(c.red, c.green, c.blue, alpha)
}
