override BLUR_Y: bool = false;
override BLOOM_SCALE: f32 = 1.0;

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct FullOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> FullOut {
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    var out: FullOut;
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x * 0.5 + 0.5, 0.5 - y * 0.5);
    return out;
}

@fragment
fn fs_down(in: FullOut) -> @location(0) vec4<f32> {
    let t = 1.0 / vec2<f32>(textureDimensions(src));
    var c = vec3<f32>(0.0);
    c += textureSampleLevel(src, samp, in.uv + vec2<f32>(-t.x, -t.y), 0.0).rgb;
    c += textureSampleLevel(src, samp, in.uv + vec2<f32>(t.x, -t.y), 0.0).rgb;
    c += textureSampleLevel(src, samp, in.uv + vec2<f32>(-t.x, t.y), 0.0).rgb;
    c += textureSampleLevel(src, samp, in.uv + vec2<f32>(t.x, t.y), 0.0).rgb;
    c *= 0.25;
    let lum = dot(c, vec3<f32>(0.299, 0.587, 0.114));
    return vec4<f32>(c * lum, 1.0);
}

@fragment
fn fs_blur(in: FullOut) -> @location(0) vec4<f32> {
    let t = 1.0 / vec2<f32>(textureDimensions(src));
    let step = select(vec2<f32>(t.x, 0.0), vec2<f32>(0.0, t.y), BLUR_Y);
    var sum = textureSampleLevel(src, samp, in.uv, 0.0).rgb * 0.39894;
    var w = 0.39894;
    for (var k = 1; k <= 4; k++) {
        let g = 0.39894 * exp(-0.5 * f32(k * k));
        sum += (textureSampleLevel(src, samp, in.uv + step * f32(k), 0.0).rgb
            + textureSampleLevel(src, samp, in.uv - step * f32(k), 0.0).rgb) * g;
        w += 2.0 * g;
    }
    return vec4<f32>(sum / w, 1.0);
}

@fragment
fn fs_add(in: FullOut) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSampleLevel(src, samp, in.uv, 0.0).rgb * BLOOM_SCALE, 0.0);
}
