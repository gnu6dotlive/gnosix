struct FrameParams {
    primary: vec4<f32>,
    control: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> params: FrameParams;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );

    var out: VertexOut;
    let p = positions[vertex_index];
    out.position = vec4<f32>(p, 0.0, 1.0);
    out.uv = p * 0.5 + vec2<f32>(0.5, 0.5);
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    if params.control.y < 0.5 {
        return params.primary;
    }

    let t = params.control.x;
    let p = in.uv - vec2<f32>(0.5, 0.5);
    let radial = length(p);
    let wave = 0.5 + 0.5 * sin((p.x * 7.0 + p.y * 4.0) + t * 0.65);
    let pulse = 0.5 + 0.5 * cos(radial * 13.0 - t * 0.45);

    let base = vec3<f32>(0.025, 0.035, 0.060);
    let cool = vec3<f32>(0.070, 0.160, 0.245);
    let warm = vec3<f32>(0.930, 0.300, 0.060);
    let color = base + cool * wave * 0.55 + warm * pulse * 0.16;

    return vec4<f32>(color, 1.0);
}
