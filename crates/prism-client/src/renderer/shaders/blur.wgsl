// Separable two-pass Gaussian blur.
// Pass 1 (horizontal): direction = vec2(1/width,  0)
// Pass 2 (vertical):   direction = vec2(0, 1/height)
//
// Uses 5-tap linear sampling (each tap covers two texels) → equivalent
// to a 9-tap Gaussian kernel.

struct BlurUniforms {
    // Texel-step in UV space: (1/w, 0) or (0, 1/h)
    direction: vec2<f32>,
}

@group(0) @binding(0) var source_tex:    texture_2d<f32>;
@group(0) @binding(1) var source_samp:   sampler;
@group(0) @binding(2) var<uniform> blur: BlurUniforms;

// ── Vertex ────────────────────────────────────────────────────────────────────
// Full-screen triangle: no vertex buffer needed.
// Vertices live at clip-space positions that cover the entire viewport.

struct VsOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0)       uv:       vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VsOutput {
    // Bit tricks to generate:  uv  in [0,1]² from vertex index 0,1,2
    //   vid=0 → (0,0), vid=1 → (2,0), vid=2 → (0,2)
    let uv = vec2<f32>(
        f32((vid << 1u) & 2u),
        f32( vid        & 2u),
    );
    // NDC: uv (0→1) → clip (-1→1), flip Y
    let clip = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);

    var out: VsOutput;
    out.clip_pos = vec4<f32>(clip, 0.0, 1.0);
    out.uv       = uv;
    return out;
}

// ── Fragment ──────────────────────────────────────────────────────────────────
// 9-tap equivalent Gaussian via 5 linear-sampled taps.

const WEIGHTS = array<f32, 5>(
    0.2270270270,
    0.3162162162,
    0.0702702703,
    0.0031351351,
    0.0000762601,
);

const OFFSETS = array<f32, 5>(
    0.0,
    1.3846153846,
    3.2307692308,
    5.076923077,
    6.923076923,
);

@fragment
fn fs_main(in: VsOutput) -> @location(0) vec4<f32> {
    var color = textureSample(source_tex, source_samp, in.uv) * WEIGHTS[0];

    for (var i: i32 = 1; i < 5; i = i + 1) {
        let off = blur.direction * OFFSETS[i];
        color += textureSample(source_tex, source_samp, in.uv + off) * WEIGHTS[i];
        color += textureSample(source_tex, source_samp, in.uv - off) * WEIGHTS[i];
    }

    return color;
}
