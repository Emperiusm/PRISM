// Glyph atlas text rendering fragment shader.
// Paired with quad.wgsl for vertex stage.
// Each instance in the quad storage buffer maps one glyph rectangle to its
// atlas UV rect.  Per-instance color comes from a separate material buffer.

struct TextMaterial {
    color: vec4<f32>,
}

@group(0) @binding(0) var atlas_tex:              texture_2d<f32>;
@group(0) @binding(1) var atlas_samp:             sampler;
@group(0) @binding(2) var<storage, read> materials: array<TextMaterial>;

struct FragInput {
    @builtin(position)              frag_coord:  vec4<f32>,
    @location(0)                    uv:          vec2<f32>,
    @location(1)                    screen_pos:  vec2<f32>,
    @location(2) @interpolate(flat) instance_id: u32,
}

@fragment
fn fs_main(in: FragInput) -> @location(0) vec4<f32> {
    let mat = materials[in.instance_id];

    // Red channel of the atlas stores SDF / coverage mask.
    let coverage = textureSample(atlas_tex, atlas_samp, in.uv).r;

    return vec4<f32>(mat.color.rgb, mat.color.a * coverage);
}
