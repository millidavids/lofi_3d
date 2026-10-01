// Water: shallow-to-deep colour from depth below the surface, foam at the
// shore, a sun highlight. Ground height comes per vertex from the CPU (uv.x)
// and depth is the sea level minus it, so it doesn't change with view angle
// and needs no depth prepass.
#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip,
    mesh_view_bindings::{view, lights, globals},
}
#import lofi::fog::{FogParams, apply_fog}

const PI: f32 = 3.141592653589793;

struct WaterParams {
    // Linear RGB; alpha is opacity.
    shallow: vec4<f32>,
    deep: vec4<f32>,
    // Sky ambient, display units.
    ambient: vec4<f32>,
    fog: FogParams,
    // Metres until the water reads fully deep.
    depth_range: f32,
    level: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: WaterParams;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

struct Varyings {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) depth: f32,
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
    out.depth = params.level - v.uv.x;
    return out;
}

@fragment
fn fragment(in: Varyings) -> @location(0) vec4<f32> {
    if in.depth <= 0.0 {
        discard;
    }
    let t = saturate(in.depth / params.depth_range);
    var color = mix(params.shallow.rgb, params.deep.rgb, t);
    var alpha = mix(params.shallow.a, params.deep.a, t);

    // Foam: a band along the shore, broken up by two crossing ripples.
    let xz = in.world_position.xz;
    let time = globals.time;
    let ripple = sin(xz.x * 1.7 + time * 1.3) * sin(xz.y * 1.3 - time * 1.1) * 0.5 + 0.5;
    let shore = 1.0 - saturate(in.depth / 0.7);
    let foam = shore * smoothstep(0.3, 0.8, ripple + shore * 0.6);

    var lit = params.ambient.rgb;
    var highlight = vec3<f32>(0.0);
    if lights.n_directional_lights > 0u {
        let light = &lights.directional_lights[0];
        let l = (*light).direction_to_light;
        let sun = (*light).color.rgb / PI * view.exposure;
        lit += sun * max(l.y, 0.0);
        let to_eye = normalize(view.world_position - in.world_position.xyz);
        let half_way = normalize(l + to_eye);
        highlight = sun * pow(max(half_way.y, 0.0), 120.0) * 1.5;
    }
    color = mix(color * lit, vec3<f32>(0.92, 0.95, 0.97) * lit, foam) + highlight;
    alpha = max(alpha, foam);
    color = apply_fog(params.fog, color, in.world_position.xyz, view.world_position);
    return vec4<f32>(color, alpha);
}
