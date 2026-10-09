// `shading.srgb_mode = 1` with MSAA: the samples averaged as the values the
// image stores. In that mode the stored value is the lit value itself (I/F
// times the exposure), and the shader writes it decoded through sRGB for the
// target to encode back; the hardware resolve averages the decoded values,
// so a pixel half lit and half dark stored more than half the light -- 13 %
// over on a 2 px Phobos. Here each sample is encoded first, the encoded
// values averaged, and the mean decoded for the target to store as it is.

@group(0) @binding(0)
var samples: texture_multisampled_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    // One triangle over the whole target.
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

fn encode(c: vec3<f32>) -> vec3<f32> {
    let low = c * 12.92;
    let high = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(high, low, c <= vec3<f32>(0.0031308));
}

fn decode(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((max(c, vec3<f32>(0.0)) + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_main(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(at.xy);
    let n = textureNumSamples(samples);
    var sum = vec4<f32>(0.0);
    for (var i = 0u; i < n; i++) {
        let s = textureLoad(samples, p, i32(i));
        sum += vec4<f32>(encode(s.rgb), s.a);
    }
    let mean = sum / f32(n);
    return vec4<f32>(decode(mean.rgb), mean.a);
}
