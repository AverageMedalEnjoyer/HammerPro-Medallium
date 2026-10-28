@group(1) @binding(0) var mask: texture_2d<f32>;

@vertex
fn vs_outline(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let x = f32((vi << 1u) & 2u);
    let y = f32(vi & 2u);
    return vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.5, 1.0);
}

@fragment
fn fs_outline(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(frag.xy);
    let dims = vec2<i32>(textureDimensions(mask));
    if textureLoad(mask, clamp(p, vec2<i32>(0), dims - vec2<i32>(1)), 0).r > 0.5 {
        discard;
    }
    var near = 0.0;
    for (var dy = -2; dy <= 2; dy++) {
        for (var dx = -2; dx <= 2; dx++) {
            if dx * dx + dy * dy > 5 {
                continue;
            }
            let q = clamp(p + vec2<i32>(dx, dy), vec2<i32>(0), dims - vec2<i32>(1));
            near = max(near, textureLoad(mask, q, 0).r);
        }
    }
    if near < 0.5 {
        discard;
    }
    return vec4<f32>(view.selection.rgb, 1.0);
}
