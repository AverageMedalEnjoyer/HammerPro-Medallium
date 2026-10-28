struct Candidate {
    aabb_min: vec3<f32>,
    first: u32,
    aabb_max: vec3<f32>,
    count: u32,
    chunk: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
};

struct CullParams {
    view_proj: mat4x4<f32>,
    size: vec2<f32>,
    mips: u32,
    count: u32,
};

@group(0) @binding(0) var depth_ms: texture_depth_multisampled_2d;
@group(0) @binding(1) var hiz_out: texture_storage_2d<r32float, write>;

@compute @workgroup_size(8, 8)
fn hiz_init_ms(@builtin(global_invocation_id) id: vec3<u32>) {
    let out_size = textureDimensions(hiz_out);
    if id.x >= out_size.x || id.y >= out_size.y {
        return;
    }
    let src = textureDimensions(depth_ms);
    let n = textureNumSamples(depth_ms);
    var far = 1.0;
    for (var dy = 0u; dy < 2u; dy++) {
        for (var dx = 0u; dx < 2u; dx++) {
            let p = min(id.xy * 2u + vec2<u32>(dx, dy), src - vec2<u32>(1u));
            for (var s = 0u; s < n; s++) {
                far = min(far, textureLoad(depth_ms, p, i32(s)));
            }
        }
    }
    textureStore(hiz_out, id.xy, vec4<f32>(far, 0.0, 0.0, 0.0));
}

@group(0) @binding(2) var depth_ss: texture_depth_2d;

@compute @workgroup_size(8, 8)
fn hiz_init(@builtin(global_invocation_id) id: vec3<u32>) {
    let out_size = textureDimensions(hiz_out);
    if id.x >= out_size.x || id.y >= out_size.y {
        return;
    }
    let src = textureDimensions(depth_ss);
    var far = 1.0;
    for (var dy = 0u; dy < 2u; dy++) {
        for (var dx = 0u; dx < 2u; dx++) {
            let p = min(id.xy * 2u + vec2<u32>(dx, dy), src - vec2<u32>(1u));
            far = min(far, textureLoad(depth_ss, p, 0));
        }
    }
    textureStore(hiz_out, id.xy, vec4<f32>(far, 0.0, 0.0, 0.0));
}

@group(0) @binding(3) var hiz_in: texture_2d<f32>;

@compute @workgroup_size(8, 8)
fn hiz_down(@builtin(global_invocation_id) id: vec3<u32>) {
    let out_size = textureDimensions(hiz_out);
    if id.x >= out_size.x || id.y >= out_size.y {
        return;
    }
    let src = textureDimensions(hiz_in);
    var far = 1.0;
    for (var dy = 0u; dy < 3u; dy++) {
        for (var dx = 0u; dx < 3u; dx++) {
            if (dx == 2u && src.x <= out_size.x * 2u) || (dy == 2u && src.y <= out_size.y * 2u) {
                continue;
            }
            let p = min(id.xy * 2u + vec2<u32>(dx, dy), src - vec2<u32>(1u));
            far = min(far, textureLoad(hiz_in, p, 0).x);
        }
    }
    textureStore(hiz_out, id.xy, vec4<f32>(far, 0.0, 0.0, 0.0));
}

@group(0) @binding(4) var<uniform> cull: CullParams;
@group(0) @binding(5) var<storage, read> candidates: array<Candidate>;
@group(0) @binding(6) var<storage, read_write> visible: array<u32>;
@group(0) @binding(7) var<storage, read_write> args: array<u32>;
@group(0) @binding(8) var hiz: texture_2d<f32>;
@group(0) @binding(9) var<storage, read_write> stats: array<atomic<u32>>;

fn write_args(i: u32, c: Candidate, draw: bool) {
    let o = i * 5u;
    args[o] = c.count;
    args[o + 1u] = select(0u, 1u, draw);
    args[o + 2u] = c.first;
    args[o + 3u] = 0u;
    args[o + 4u] = 0u;
}

@compute @workgroup_size(64)
fn cull_prepare(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if i >= cull.count || i >= arrayLength(&candidates) {
        return;
    }
    let c = candidates[i];
    var was = 0u;
    if c.chunk < arrayLength(&visible) {
        was = visible[c.chunk];
    }
    write_args(i, c, was != 0u);
}

fn box_visible(c: Candidate) -> bool {
    var lo = vec2<f32>(1e9);
    var hi = vec2<f32>(-1e9);
    var nearest = 0.0;
    for (var k = 0u; k < 8u; k++) {
        let p = vec3<f32>(
            select(c.aabb_min.x, c.aabb_max.x, (k & 1u) != 0u),
            select(c.aabb_min.y, c.aabb_max.y, (k & 2u) != 0u),
            select(c.aabb_min.z, c.aabb_max.z, (k & 4u) != 0u),
        );
        let clip = cull.view_proj * vec4<f32>(p, 1.0);
        if clip.w <= 1e-4 {
            return true;
        }
        let ndc = clip.xyz / clip.w;
        lo = min(lo, ndc.xy);
        hi = max(hi, ndc.xy);
        nearest = max(nearest, ndc.z);
    }
    if nearest >= 1.0 {
        return true;
    }
    let uv_lo = vec2<f32>(lo.x * 0.5 + 0.5, 0.5 - hi.y * 0.5);
    let uv_hi = vec2<f32>(hi.x * 0.5 + 0.5, 0.5 - lo.y * 0.5);
    let t_lo = clamp(uv_lo, vec2<f32>(0.0), vec2<f32>(1.0)) * cull.size;
    let t_hi = clamp(uv_hi, vec2<f32>(0.0), vec2<f32>(1.0)) * cull.size;
    let extent = max(t_hi.x - t_lo.x, t_hi.y - t_lo.y);
    let mip = min(u32(ceil(log2(max(extent, 1.0)))), cull.mips - 1u);
    let dims = vec2<i32>(textureDimensions(hiz, i32(mip)));
    let scale = 1.0 / f32(1u << mip);
    let a = clamp(vec2<i32>(floor(t_lo * scale)), vec2<i32>(0), dims - vec2<i32>(1));
    let b = clamp(vec2<i32>(floor(t_hi * scale)), vec2<i32>(0), dims - vec2<i32>(1));
    var far = 1.0;
    for (var y = a.y; y <= b.y; y++) {
        for (var x = a.x; x <= b.x; x++) {
            far = min(far, textureLoad(hiz, vec2<i32>(x, y), i32(mip)).x);
        }
    }
    return nearest >= far;
}

@compute @workgroup_size(64)
fn cull_test(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if i >= cull.count || i >= arrayLength(&candidates) {
        return;
    }
    let c = candidates[i];
    let now = box_visible(c);
    var was = 0u;
    if c.chunk < arrayLength(&visible) {
        was = visible[c.chunk];
        visible[c.chunk] = select(0u, 1u, now);
    }
    write_args(i, c, now && was == 0u);
    if !now {
        atomicAdd(&stats[0], 1u);
    }
}
