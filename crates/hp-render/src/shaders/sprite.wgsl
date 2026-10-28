struct SpriteOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) flags: u32,
};

var<private> CORNERS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>(1.0, -1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(-1.0, -1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(-1.0, 1.0),
);

@vertex
fn vs_sprite(
    @builtin(vertex_index) vi: u32,
    @location(0) pos: vec3<f32>,
    @location(1) size: f32,
    @location(2) color: vec4<f32>,
    @location(3) slot: u32,
) -> SpriteOut {
    let st = object_state(slot);
    var out: SpriteOut;
    let c = CORNERS[vi % 6u];
    out.uv = vec2<f32>(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5);
    out.color = vec4<f32>(srgb_to_linear(color.rgb), color.a);
    out.flags = st.flags;
    if hidden_in_view(st.flags) {
        out.clip = COLLAPSED;
        return out;
    }
    var half = max(size, 0.0) * 0.5;
    if is_ortho() {
        half = f32(view.opts.y) * 0.5 / max(view.grid_b.z, 1e-6);
    }
    let world = pos + (view.right.xyz * c.x + view.up.xyz * c.y) * half;
    out.clip = view.view_proj * vec4<f32>(world, 1.0);
    return out;
}

@fragment
fn fs_sprite(in: SpriteOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp_clamp, in.uv) * material.tint * in.color;
    if t.a < 0.02 {
        discard;
    }
    var rgb = t.rgb;
    if (in.flags & F_SELECTED) != 0u {
        rgb = mix(rgb, view.selection.rgb, max(view.selection.a, 0.5));
    }
    if (in.flags & F_HOVERED) != 0u {
        rgb = mix(rgb, vec3<f32>(1.0), 0.25);
    }
    return vec4<f32>(rgb, t.a);
}
