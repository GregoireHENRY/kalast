struct Globals {
    color: vec3<f32>,
    color_mode: u32,
    srgb_mode: u32,
    gamma: f32,
    ambient_strength: f32,
    light_cube_scale: f32,
    shadow_resolution: u32,
    shadow_bias_scale: f32,
    shadow_bias_minimum: f32,
    shadow_normal_offset_scale: f32,
    shadow_pcf: u32,
    extra: u32,
};
@group(0) @binding(0)
var<uniform> globals: Globals;

struct Camera {
    view_proj: mat4x4<f32>,
};

struct Light {
    view_proj: mat4x4<f32>,
    pos: vec3<f32>,
    color: vec3<f32>,
};

struct View {
    camera: Camera,
    light: Light,
};
@group(1) @binding(0)
var<uniform> view: View;

struct InstanceInput {
    @location(8) mat_row_0: vec4<f32>,
    @location(9) mat_row_1: vec4<f32>,
    @location(10) mat_row_2: vec4<f32>,
    @location(11) mat_row_3: vec4<f32>,
    @location(12) normal_row_0: vec4<f32>,
    @location(13) normal_row_1: vec4<f32>,
    @location(14) normal_row_2: vec4<f32>,
    @location(15) normal_row_3: vec4<f32>,
    // @location(16) color: vec3<f32>,
    // @location(17) color_mode: u32,
};

struct VertexInput {
    @location(0) pos: vec3<f32>,
    @location(1) tex: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) tangent: vec3<f32>,
    @location(4) bitangent: vec3<f32>,
    @location(5) color: vec3<f32>,
    @location(6) color_mode: u32,
    @location(7) extra: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) world_normal: vec3<f32>,
    @location(3) world_pos: vec3<f32>,
};

fn srgb_to_linear(color: vec3<f32>, gamma: f32) -> vec3<f32> {
    return pow(color, vec3<f32>(gamma));
}

@vertex
fn vs_main(
    vertex: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    let model_matrix = mat4x4<f32>(
        instance.mat_row_0,
        instance.mat_row_1,
        instance.mat_row_2,
        instance.mat_row_3,
    );

    let normal_matrix = mat3x3<f32>(
        instance.normal_row_0.xyz,
        instance.normal_row_1.xyz,
        instance.normal_row_2.xyz,
    );

    var out: VertexOutput;
    out.tex = vertex.tex;

    // if instance.color_mode == 0 {
    //     out.color = vertex.color;
    // } else {
    //     out.color = instance.color;
    // }

    out.color = vertex.color;

    out.world_normal = normalize(normal_matrix * vertex.normal);

    var world_pos = model_matrix * vec4<f32>(vertex.pos, 1.0);
    out.world_pos = world_pos.xyz;

    out.clip_position = view.camera.view_proj * world_pos;

    return out;
}

@group(2) @binding(0)
var t_shadow: texture_depth_2d;
@group(2) @binding(1)
var s_shadow: sampler_comparison;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if globals.color_mode == 1 {
        var color = in.color;
        if globals.srgb_mode == 0 {
            color = srgb_to_linear(color, globals.gamma);
        }
        return vec4<f32>(color, 1.0);
    } else if globals.color_mode == 2 {
        var color = globals.color;
        if globals.srgb_mode == 0 {
            color = srgb_to_linear(color, globals.gamma);
        }
        return vec4<f32>(color, 1.0);
    }
    // } else if globals.color_mode == ??? {
    // object_color = textureSample(t_diffuse, s_diffuse, in.tex);

    // 0 or else
    //
    // else {
    let object_color = vec4<f32>(in.color, 1.0);

    let light_dir = normalize(view.light.pos - in.world_pos);
    let ndotl = max(dot(in.world_normal, light_dir), 0.0);
    let k = 1.0 - ndotl;
    let k2 = k * k;

    // shadow
    let normal_offset = globals.shadow_normal_offset_scale * k;
    let offset_pos = in.world_pos + in.world_normal * normal_offset;
    let light_space = view.light.view_proj * vec4<f32>(offset_pos, 1.0);
    var proj = light_space.xyz / light_space.w;
    proj.y = -proj.y;
    let uv = proj.xy * 0.5 + 0.5;
    let depth = proj.z;
    let bias = max(globals.shadow_bias_scale * k2, globals.shadow_bias_minimum);

    var shadow = 1.0;

    if globals.shadow_pcf == 0 {
        shadow = textureSampleCompare(
            t_shadow,
            s_shadow,
            uv,
            depth - bias
        );
    }
    else {
        let texel_size = 1.0 / vec2<f32>(f32(globals.shadow_resolution));
        for (var x = -i32(globals.shadow_pcf); x <= i32(globals.shadow_pcf); x++) {
            for (var y = -i32(globals.shadow_pcf); y <= i32(globals.shadow_pcf); y++) {
                let offset = vec2<f32>(f32(x), f32(y)) * texel_size;
                shadow += textureSampleCompare(t_shadow, s_shadow, uv + offset, depth - bias);
            }
        }
        shadow /= pow(f32(globals.shadow_pcf * 2 + 1), 2.0);
    }

    // no shadow
    if globals.color_mode == 3 {
        shadow = 1.0;
    }

    // lighting
    let ambient_color = view.light.color * globals.ambient_strength;
    let diffuse_color = view.light.color * ndotl;
    var color = (ambient_color + diffuse_color * shadow) * object_color.xyz;
    
    if globals.srgb_mode == 1 {
        color = srgb_to_linear(color, globals.gamma);
    }

    return vec4<f32>(color, object_color.a);
}