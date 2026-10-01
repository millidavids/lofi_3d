// Terrain: up to four tiling materials per chunk, height-blended, lit like
// WoW Classic: soft sun, sky/ground ambient, no specular.
//
// Mesh attributes (see view::terrain_mesh):
// - uv:   the chunk's four material layers, two per float (a + b * 256)
// - uv_b: tint and ambient occlusion bytes, two per float (r + g * 256, b + ao * 256)
// - color: the four slot weights
#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip,
    mesh_view_bindings::{view, lights},
    mesh_view_types,
    shadows,
}
#import lofi::fog::{FogParams, apply_fog}

const PI: f32 = 3.141592653589793;
const FLAG_FLAT: u32 = 1u;
const FLAG_CRISP: u32 = 2u;
const FLAG_SLOPE: u32 = 4u;
const FLAG_CHUNK_GRID: u32 = 8u;
const SLOPE_COLOR: vec3<f32> = vec3<f32>(0.85, 0.12, 0.08);
const GRID_COLOR: vec3<f32> = vec3<f32>(1.0, 0.85, 0.2);
const LAST_LAYER: u32 = 31u;
// How far one material's height can reach into another's weight.
const BLEND_DEPTH: f32 = 0.25;
// A second, larger and rotated sample of each material shades the first, so
// the tile's repeat doesn't read as rows at grazing angles.
const MACRO_SCALE: f32 = 0.181;
const MACRO_ROTATION: mat2x2<f32> = mat2x2<f32>(0.799, 0.602, -0.602, 0.799);
const MACRO_STRENGTH: f32 = 0.6;

struct TerrainParams {
    // Linear RGB per material layer; w is 1 / tile size in metres.
    colors: array<vec4<f32>, 32>,
    sky_ambient: vec4<f32>,
    ground_ambient: vec4<f32>,
    fog: FogParams,
    flags: u32,
    texture_size: f32,
    // Metres per chunk side, for the grid overlay.
    chunk_size: f32,
    // Cosine of the steepest walkable slope, for the slope overlay.
    walkable_cos: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: TerrainParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var textures: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var textures_sampler: sampler;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) layers: vec2<f32>,
    @location(3) tint_ao: vec2<f32>,
    @location(5) weights: vec4<f32>,
}

struct Varyings {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) weights: vec4<f32>,
    @location(3) @interpolate(flat) layers: vec4<u32>,
    @location(4) tint: vec3<f32>,
    @location(5) ao: f32,
}

fn unpack_pair(v: f32) -> vec2<u32> {
    let packed = u32(round(v));
    return vec2<u32>(packed & 255u, packed >> 8u);
}

@vertex
fn vertex(v: Vertex) -> Varyings {
    var out: Varyings;
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    out.world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(v.position, 1.0),
    );
    out.clip_position = position_world_to_clip(out.world_position.xyz);
    out.world_normal = mesh_functions::mesh_normal_local_to_world(v.normal, v.instance_index);
    out.weights = v.weights;
    let a = unpack_pair(v.layers.x);
    let b = unpack_pair(v.layers.y);
    out.layers = min(vec4<u32>(a.x, a.y, b.x, b.y), vec4<u32>(LAST_LAYER));
    let rg = unpack_pair(v.tint_ao.x);
    let ba = unpack_pair(v.tint_ao.y);
    // 128 is neutral, so tint can brighten as well as darken.
    out.tint = vec3<f32>(f32(rg.x), f32(rg.y), f32(ba.x)) / 128.0;
    out.ao = f32(ba.y) / 255.0;
    return out;
}

// Snaps to texel centres but blends a pixel-wide seam, so close-up texels
// read as crisp squares without shimmering when far away. `width` is the
// UV's fwidth.
fn crisp(uv: vec2<f32>, width: vec2<f32>) -> vec2<f32> {
    let texel = uv * params.texture_size;
    let seam = floor(texel + 0.5);
    let texels = max(width * params.texture_size, vec2<f32>(0.00001));
    return (seam + clamp((texel - seam) / texels, vec2<f32>(-0.5), vec2<f32>(0.5)))
        / params.texture_size;
}

// Albedo in rgb, blend height in a. Takes world XZ's screen derivatives
// rather than computing its own, so it can run in a per-pixel branch.
fn sample_layer(layer: u32, xz: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>) -> vec4<f32> {
    let scale = params.colors[layer].w;
    let uv = xz * scale;
    let ux = dx * scale;
    let uy = dy * scale;
    var at = uv;
    if (params.flags & FLAG_CRISP) != 0u {
        at = crisp(uv, abs(ux) + abs(uy));
    }
    let detail = textureSampleGrad(textures, textures_sampler, at, layer, ux, uy);
    let big = MACRO_ROTATION * uv * MACRO_SCALE;
    let bx = MACRO_ROTATION * ux * MACRO_SCALE;
    let by = MACRO_ROTATION * uy * MACRO_SCALE;
    let shade = textureSampleGrad(textures, textures_sampler, big, layer, bx, by);
    let variation = mix(1.0, dot(shade.rgb, vec3<f32>(2.0 / 3.0)), MACRO_STRENGTH);
    // Detail is centred on 0.5, so doubling it keeps the material's colour.
    return vec4<f32>(params.colors[layer].rgb * detail.rgb * 2.0 * variation, detail.a);
}

fn sun_light(n: vec3<f32>, world_position: vec4<f32>, frag_xy: vec2<f32>) -> vec3<f32> {
    if lights.n_directional_lights == 0u {
        return vec3<f32>(0.0);
    }
    let light = &lights.directional_lights[0];
    let ndotl = max(dot(n, (*light).direction_to_light), 0.0);
    var shadow = 1.0;
    if ((*light).flags & mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u {
        let view_z = dot(vec4<f32>(
            view.view_from_world[0].z,
            view.view_from_world[1].z,
            view.view_from_world[2].z,
            view.view_from_world[3].z,
        ), world_position);
        shadow = shadows::fetch_directional_shadow(0u, world_position, n, view_z, frag_xy);
    }
    // Lambert. Light colour carries illuminance, which exposure maps to display.
    return (*light).color.rgb * ndotl * shadow / PI * view.exposure;
}

@fragment
fn fragment(in: Varyings) -> @location(0) vec4<f32> {
    let xz = in.world_position.xz;
    let dx = dpdx(xz);
    let dy = dpdy(xz);
    // Unpainted slots are skipped: most chunks blend fewer than four.
    let w = in.weights;
    let painted = step(vec4<f32>(0.004), w);
    var s0 = vec4<f32>(0.0);
    var s1 = vec4<f32>(0.0);
    var s2 = vec4<f32>(0.0);
    var s3 = vec4<f32>(0.0);
    if painted.x > 0.0 {
        s0 = sample_layer(in.layers.x, xz, dx, dy);
    }
    if painted.y > 0.0 {
        s1 = sample_layer(in.layers.y, xz, dx, dy);
    }
    if painted.z > 0.0 {
        s2 = sample_layer(in.layers.z, xz, dx, dy);
    }
    if painted.w > 0.0 {
        s3 = sample_layer(in.layers.w, xz, dx, dy);
    }

    // Height blend: where two materials meet, the taller texel wins, so sand
    // settles into cracks instead of smearing over rock. Only painted slots
    // compete; an unpainted one sits at -1, below anything painted.
    let h = (vec4<f32>(s0.a, s1.a, s2.a, s3.a) + w) * painted - (1.0 - painted);
    let top = max(max(h.x, h.y), max(h.z, h.w)) - BLEND_DEPTH;
    let b = max(h - vec4<f32>(top), vec4<f32>(0.0)) * painted;
    let albedo = (s0.rgb * b.x + s1.rgb * b.y + s2.rgb * b.z + s3.rgb * b.w)
        / max(b.x + b.y + b.z + b.w, 0.0001);

    var n = normalize(in.world_normal);
    if (params.flags & FLAG_FLAT) != 0u {
        let p = in.world_position.xyz;
        n = normalize(cross(dpdy(p), dpdx(p)));
    }

    let sun = sun_light(n, in.world_position, in.clip_position.xy);
    let ambient = mix(params.ground_ambient.rgb, params.sky_ambient.rgb, n.y * 0.5 + 0.5) * in.ao;
    var color = albedo * in.tint * (sun + ambient);
    color = apply_fog(params.fog, color, in.world_position.xyz, view.world_position);

    // Overlays sit on top of the fog, so they read at any distance.
    if (params.flags & FLAG_SLOPE) != 0u {
        // The face a character would stand on, not the smoothed normal.
        let p = in.world_position.xyz;
        let face = normalize(cross(dpdy(p), dpdx(p)));
        if face.y < params.walkable_cos {
            color = mix(color, SLOPE_COLOR, 0.55);
        }
    }
    if (params.flags & FLAG_CHUNK_GRID) != 0u {
        let g = xz / params.chunk_size;
        // Distance to the nearest border in pixels, for a one-pixel line.
        let pixels = abs(fract(g - 0.5) - 0.5) / max(fwidth(g), vec2<f32>(1e-6));
        let line = 1.0 - min(min(pixels.x, pixels.y), 1.0);
        color = mix(color, GRID_COLOR, line * 0.85);
    }
    return vec4<f32>(color, 1.0);
}
