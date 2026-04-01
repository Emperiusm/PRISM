// Accent glow rectangle fragment shader.
// Paired with quad.wgsl for vertex stage.
// Produces a soft radial falloff from the rectangle centre.

struct GlowMaterial {
    color:     vec4<f32>,
    spread:    f32,
    intensity: f32,
    // padding to 16-byte alignment
    _pad0:     f32,
    _pad1:     f32,
}

@group(0) @binding(0) var<storage, read> materials: array<GlowMaterial>;

struct FragInput {
    @builtin(position)              frag_coord:  vec4<f32>,
    @location(0)                    uv:          vec2<f32>,
    @location(1)                    screen_pos:  vec2<f32>,
    @location(2) @interpolate(flat) instance_id: u32,
}

@fragment
fn fs_main(in: FragInput) -> @location(0) vec4<f32> {
    let mat = materials[in.instance_id];

    // Distance from centre in UV space [0,1]²; max distance = ~0.707 at corners
    let centered = in.uv - vec2<f32>(0.5, 0.5);
    let dist     = length(centered);

    // Exponential radial falloff
    let falloff = exp(-dist * mat.spread) * mat.intensity;

    let alpha = clamp(falloff * mat.color.a, 0.0, 1.0);
    return vec4<f32>(mat.color.rgb, alpha);
}
