// Fog shared by every lofi surface: terrain, water and sky fade the same way.
#define_import_path lofi::fog

struct FogParams {
    // Linear RGB; alpha 0 turns fog off.
    color: vec4<f32>,
    // Start and end of linear fog in metres, then height fog density and
    // falloff per metre.
    distance: vec4<f32>,
}

// How much fog lies between the camera and a point, 0..1.
fn fog_amount(fog: FogParams, world: vec3<f32>, camera: vec3<f32>) -> f32 {
    if fog.color.a <= 0.0 {
        return 0.0;
    }
    let to_point = world - camera;
    let dist = length(to_point);
    let span = max(fog.distance.y - fog.distance.x, 0.001);
    let linear = saturate((dist - fog.distance.x) / span);

    // Density falling off exponentially with height, integrated along the
    // view ray (Inigo Quilez, "Better Fog").
    var height = 0.0;
    let density = fog.distance.z;
    if density > 0.0 {
        let falloff = fog.distance.w;
        let ray_y = to_point.y / max(dist, 0.0001);
        var integral = density * exp(-falloff * camera.y) * dist;
        if abs(ray_y) > 0.0001 {
            integral = density / falloff * exp(-falloff * camera.y)
                * (1.0 - exp(-falloff * ray_y * dist)) / ray_y;
        }
        height = 1.0 - exp(-max(integral, 0.0));
    }
    return saturate(max(linear, height)) * fog.color.a;
}

fn apply_fog(fog: FogParams, color: vec3<f32>, world: vec3<f32>, camera: vec3<f32>) -> vec3<f32> {
    return mix(color, fog.color.rgb, fog_amount(fog, world, camera));
}
