// Frosted-glass panel fragment shader.
// Paired with quad.wgsl for vertex stage.

struct GlassMaterial {
    tint:           vec4<f32>,
    border_color:   vec4<f32>,
    corner_radius:  f32,
    noise_intensity: f32,
    panel_width:    f32,
    panel_height:   f32,
}

struct ScreenUniforms {
    screen_size: vec2<f32>,
}

@group(0) @binding(0) var blur_tex:      texture_2d<f32>;
@group(0) @binding(1) var blur_samp:     sampler;
@group(0) @binding(2) var noise_tex:     texture_2d<f32>;
@group(0) @binding(3) var noise_samp:    sampler;
@group(0) @binding(4) var<storage, read> materials: array<GlassMaterial>;
@group(0) @binding(5) var<uniform>       screen:    ScreenUniforms;

// ── Rounded-box SDF ────────────────────────────────────────────────────────────
// Returns the signed distance from point `p` to a rounded box of half-size `b`
// with corner radius `r`.  Negative = inside.
fn rounded_box_sdf(p: vec2<f32>, half_size: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half_size + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

struct FragInput {
    @builtin(position)              frag_coord:  vec4<f32>,
    @location(0)                    uv:          vec2<f32>,
    @location(1)                    screen_pos:  vec2<f32>,
    @location(2) @interpolate(flat) instance_id: u32,
}

@fragment
fn fs_main(in: FragInput) -> @location(0) vec4<f32> {
    let mat = materials[in.instance_id];

    let half_size = vec2<f32>(mat.panel_width * 0.5, mat.panel_height * 0.5);
    // p: position relative to panel centre in pixels
    let p = (in.uv - vec2<f32>(0.5, 0.5)) * vec2<f32>(mat.panel_width, mat.panel_height);

    let dist = rounded_box_sdf(p, half_size, mat.corner_radius);

    // Discard outside the rounded rect (0.5 px tolerance)
    if (dist > 0.5) {
        discard;
    }

    // ── Blurred background sample ─────────────────────────────────────────────
    let blur_uv = in.screen_pos / screen.screen_size;
    var color   = textureSample(blur_tex, blur_samp, blur_uv).rgb;

    // ── Tint ──────────────────────────────────────────────────────────────────
    color = mix(color, mat.tint.rgb, mat.tint.a);

    // ── Noise overlay ─────────────────────────────────────────────────────────
    let noise  = textureSample(noise_tex, noise_samp, in.uv * 4.0).r;
    color     += (noise - 0.5) * mat.noise_intensity;

    // ── Border (1 px, brighter at top) ────────────────────────────────────────
    let border_dist = abs(dist + 1.0);   // 1-px band inside the edge
    if (border_dist < 1.0) {
        let brightness = mix(1.6, 1.0, in.uv.y); // brighter at top
        let border_mix = (1.0 - border_dist) * mat.border_color.a;
        color = mix(color, mat.border_color.rgb * brightness, border_mix);
    }

    // ── Outer glow (exponential falloff just outside the rect) ───────────────
    let glow_dist  = max(dist, 0.0);
    let glow_alpha = exp(-glow_dist * 0.15) * 0.35;
    color = mix(color, mat.border_color.rgb, glow_alpha * mat.border_color.a);

    // ── Edge anti-aliasing ────────────────────────────────────────────────────
    let edge_alpha = 1.0 - smoothstep(-0.5, 0.5, dist);

    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), edge_alpha);
}
