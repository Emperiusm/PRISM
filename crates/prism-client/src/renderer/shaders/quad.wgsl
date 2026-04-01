// Instanced quad vertex shader — shared by glass, glow, and stream passes.
// Each instance describes a screen-space axis-aligned rectangle.

struct QuadInstance {
    // Pixel-space rect: x, y, width, height
    rect:    vec4<f32>,
    // UV rect: u0, v0, u1, v1
    uv_rect: vec4<f32>,
}

struct ScreenUniforms {
    screen_size: vec2<f32>,
}

@group(0) @binding(0) var<storage, read> instances: array<QuadInstance>;
@group(0) @binding(1) var<uniform>       screen:    ScreenUniforms;

struct VertexOutput {
    @builtin(position)         clip_pos:    vec4<f32>,
    @location(0)               uv:          vec2<f32>,
    @location(1)               screen_pos:  vec2<f32>,
    @location(2) @interpolate(flat) instance_id: u32,
}

// Two-triangle quad: vertex order matches strip-free indexed draw.
// Corners: (0,0) (1,0) (0,1) | (1,0) (1,1) (0,1)
const QUAD_CORNERS = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 1.0),
);

@vertex
fn vs_main(
    @builtin(vertex_index)   vert_idx: u32,
    @builtin(instance_index) inst_idx: u32,
) -> VertexOutput {
    let inst   = instances[inst_idx];
    let corner = QUAD_CORNERS[vert_idx];

    // Pixel position of this vertex
    let px = inst.rect.x + corner.x * inst.rect.z;
    let py = inst.rect.y + corner.y * inst.rect.w;

    // NDC: flip Y so +Y is up
    let ndc_x = (px / screen.screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (py / screen.screen_size.y) * 2.0;

    // Interpolate UV within uv_rect
    let uv = vec2<f32>(
        mix(inst.uv_rect.x, inst.uv_rect.z, corner.x),
        mix(inst.uv_rect.y, inst.uv_rect.w, corner.y),
    );

    var out: VertexOutput;
    out.clip_pos   = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv         = uv;
    out.screen_pos = vec2<f32>(px, py);
    out.instance_id = inst_idx;
    return out;
}
