@group(0) @binding(0) var frame: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> bins: array<atomic<u32>, 256>;

@compute @workgroup_size(16, 16)
fn histogram(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(frame);
    let p = id.xy * 2u;
    let lo = vec2<u32>(vec2<f32>(size) * vec2<f32>(0.05, 0.075));
    let hi = vec2<u32>(vec2<f32>(size) * vec2<f32>(0.95, 0.925));
    if any(p < lo) || any(p >= hi) {
        return;
    }
    let c = textureLoad(frame, vec2<i32>(p), 0).rgb;
    let lum = clamp(dot(c, vec3<f32>(0.299, 0.587, 0.114)), 0.0, 1.0);
    atomicAdd(&bins[min(u32(lum * 256.0), 255u)], 1u);
}
