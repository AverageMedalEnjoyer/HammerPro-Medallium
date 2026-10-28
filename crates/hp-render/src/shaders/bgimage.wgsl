@group(1) @binding(0) var bg_tex: texture_2d<f32>;

struct BgOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
};

@vertex
fn vs_bg(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) alpha: f32) -> BgOut {
    var out: BgOut;
    out.clip = view.view_proj * vec4<f32>(pos, 1.0);
    out.uv = uv;
    out.alpha = alpha;
    return out;
}

@fragment
fn fs_bg(in: BgOut) -> @location(0) vec4<f32> {
    let c = textureSample(bg_tex, samp_clamp, in.uv);
    return vec4<f32>(c.rgb, c.a * in.alpha);
}
