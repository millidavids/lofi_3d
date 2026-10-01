// Placed objects: flat-shaded vertex colour, lit and fogged like the terrain
// (Lambert sun with shadows, sky/ground ambient, no specular).
#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip,
    mesh_view_bindings::{view, lights},
    mesh_view_types,
    shadows,
}
#import lofi::fog::{FogParams, apply_fog}

const PI: f32 = 3.141592653589793;

struct ObjectParams {
    sky_ambient: vec4<f32>,
    ground_ambient: vec4<f32>,
    fog: FogParams,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: ObjectParams;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(5) color: vec4<f32>,
}

struct Varyings {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) color: vec3<f32>,
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
    out.color = v.color.rgb;
    return out;
}

@fragment
fn fragment(in: Varyings) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    var sun = vec3<f32>(0.0);
    if lights.n_directional_lights > 0u {
        let light = &lights.directional_lights[0];
        let ndotl = max(dot(n, (*light).direction_to_light), 0.0);
        var shadow = 1.0;
        if ((*light).flags & mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u {
            let view_z = dot(vec4<f32>(
                view.view_from_world[0].z,
                view.view_from_world[1].z,
                view.view_from_world[2].z,
                view.view_from_world[3].z,
            ), in.world_position);
            shadow = shadows::fetch_directional_shadow(
                0u,
                in.world_position,
                n,
                view_z,
                in.clip_position.xy,
            );
        }
        sun = (*light).color.rgb * ndotl * shadow / PI * view.exposure;
    }
    let ambient = mix(params.ground_ambient.rgb, params.sky_ambient.rgb, n.y * 0.5 + 0.5);
    var color = in.color * (sun + ambient);
    color = apply_fog(params.fog, color, in.world_position.xyz, view.world_position);
    return vec4<f32>(color, 1.0);
}
