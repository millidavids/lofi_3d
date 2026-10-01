// Sky: a dome around the camera, horizon to zenith, with the sun. Below the
// horizon it stays the horizon colour, which is also the fog colour, so
// distant land and sea fade straight into it.
#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip,
    mesh_view_bindings::view,
}

struct SkyParams {
    // Linear RGB.
    zenith: vec4<f32>,
    horizon: vec4<f32>,
    sun_color: vec4<f32>,
    // Towards the sun.
    sun_direction: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: SkyParams;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
}

struct Varyings {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
}

@vertex
fn vertex(v: Vertex) -> Varyings {
    var out: Varyings;
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    out.world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(v.position, 1.0),
    ).xyz;
    out.clip_position = position_world_to_clip(out.world_position);
    return out;
}

@fragment
fn fragment(in: Varyings) -> @location(0) vec4<f32> {
    let dir = normalize(in.world_position - view.world_position);
    let up = saturate(dir.y);
    var color = mix(params.horizon.rgb, params.zenith.rgb, pow(up, 0.55));
    let toward_sun = max(dot(dir, params.sun_direction.xyz), 0.0);
    let glow = pow(toward_sun, 12.0) * 0.2;
    let disc = smoothstep(0.9993, 0.9997, toward_sun);
    color += params.sun_color.rgb * (glow + disc) * step(0.0, dir.y + 0.02);
    return vec4<f32>(color, 1.0);
}
