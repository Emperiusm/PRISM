// YUV420 planar → RGBA compute shader (BT.601)
// Workgroup: 16×16 threads, each thread converts one luma sample.

struct Params {
    width:  u32,
    height: u32,
}

@group(0) @binding(0) var y_plane: texture_2d<f32>;
@group(0) @binding(1) var u_plane: texture_2d<f32>;
@group(0) @binding(2) var v_plane: texture_2d<f32>;
@group(0) @binding(3) var output:  texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(4) var<uniform> params: Params;

@compute @workgroup_size(16, 16)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }

    let y_coord = vec2<i32>(i32(gid.x), i32(gid.y));
    let uv_coord = vec2<i32>(i32(gid.x / 2u), i32(gid.y / 2u));

    let y_val = textureLoad(y_plane, y_coord, 0).r;
    let u_val = textureLoad(u_plane, uv_coord, 0).r;
    let v_val = textureLoad(v_plane, uv_coord, 0).r;

    // BT.601 limited-range offsets
    let y = y_val - (16.0 / 255.0);
    let u = u_val - 0.5;
    let v = v_val - 0.5;

    let r = clamp(y * 1.164 + v * 1.596,             0.0, 1.0);
    let g = clamp(y * 1.164 - u * 0.392 - v * 0.813, 0.0, 1.0);
    let b = clamp(y * 1.164 + u * 2.017,             0.0, 1.0);

    textureStore(output, y_coord, vec4<f32>(r, g, b, 1.0));
}
