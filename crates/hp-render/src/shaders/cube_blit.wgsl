struct BlitParams {
    rect: vec4<f32>,
    scale: vec4<f32>,
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var src_samp: sampler;
@group(0) @binding(2) var<uniform> p: BlitParams;

struct BlitOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_blit(@builtin(vertex_index) vi: u32) -> BlitOut {
    let x = f32((vi << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(vi & 2u) * 2.0 - 1.0;
    var out: BlitOut;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x * 0.5 + 0.5, 0.5 - y * 0.5);
    return out;
}

@fragment
fn fs_blit(in: BlitOut) -> @location(0) vec4<f32> {
    let uv = in.uv * p.rect.xy + p.rect.zw;
    let c = textureSampleLevel(src, src_samp, uv, 0.0);
    return vec4<f32>(max(c.rgb * p.scale.rgb, vec3<f32>(0.0)), 1.0);
}
