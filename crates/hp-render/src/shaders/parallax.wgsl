fn parallax_reflection(world: vec3<f32>, r: vec3<f32>, obb0: vec4<f32>, obb1: vec4<f32>, obb2: vec4<f32>, origin: vec4<f32>) -> vec3<f32> {
    if origin.w < 0.5 {
        return r;
    }
    let pos = vec3<f32>(dot(obb0.xyz, world) + obb0.w, dot(obb1.xyz, world) + obb1.w, dot(obb2.xyz, world) + obb2.w);
    let ray = vec3<f32>(dot(obb0.xyz, r), dot(obb1.xyz, r), dot(obb2.xyz, r));
    let first = (vec3<f32>(1.0) - pos) / ray;
    let second = -pos / ray;
    let far = max(first, second);
    let distance = min(far.x, min(far.y, far.z));
    if !(distance > 0.0) || distance > 1e30 {
        return r;
    }
    return world + r * distance - origin.xyz;
}
