#version 410 core
out vec4 frag;

in VS_OUT {
    vec3 pos;
    vec3 normal;
    vec3 color;
    float data;
    float albedo;
    flat int color_mode;

    vec4 pos_lightspace;
} fs_in;

uniform vec3 pos_light;
uniform vec3 pos_camera;

uniform vec3 directional_light_color;
uniform vec3 ambient_light_color;

uniform bool shadows;
uniform sampler2D map_shadow;
uniform float bias_acne;

uniform int colormap_code;
uniform vec2 colormap_bounds;

uniform bool force_color;
uniform vec3 forced_color;

vec3 viridis(float t) {
    const vec3 c0 = vec3(0.2777273272234177, 0.005407344544966578, 0.3340998053353061);
    const vec3 c1 = vec3(0.1050930431085774, 1.404613529898575, 1.384590162594685);
    const vec3 c2 = vec3(-0.3308618287255563, 0.214847559468213, 0.09509516302823659);
    const vec3 c3 = vec3(-4.634230498983486, -5.799100973351585, -19.33244095627987);
    const vec3 c4 = vec3(6.228269936347081, 14.17993336680509, 56.69055260068105);
    const vec3 c5 = vec3(4.776384997670288, -13.74514537774601, -65.35303263337234);
    const vec3 c6 = vec3(-5.435455855934631, 4.645852612178535, 26.3124352495832);
    return c0+t*(c1+t*(c2+t*(c3+t*(c4+t*(c5+t*c6)))));
}

vec3 plasma(float t) {
    const vec3 c0 = vec3(0.05873234392399702, 0.02333670892565664, 0.5433401826748754);
    const vec3 c1 = vec3(2.176514634195958, 0.2383834171260182, 0.7539604599784036);
    const vec3 c2 = vec3(-2.689460476458034, -7.455851135738909, 3.110799939717086);
    const vec3 c3 = vec3(6.130348345893603, 42.3461881477227, -28.51885465332158);
    const vec3 c4 = vec3(-11.10743619062271, -82.66631109428045, 60.13984767418263);
    const vec3 c5 = vec3(10.02306557647065, 71.41361770095349, -54.07218655560067);
    const vec3 c6 = vec3(-3.658713842777788, -22.93153465461149, 18.19190778539828);
    return c0+t*(c1+t*(c2+t*(c3+t*(c4+t*(c5+t*c6)))));
}

vec3 magma(float t) {
    const vec3 c0 = vec3(-0.002136485053939582, -0.000749655052795221, -0.005386127855323933);
    const vec3 c1 = vec3(0.2516605407371642, 0.6775232436837668, 2.494026599312351);
    const vec3 c2 = vec3(8.353717279216625, -3.577719514958484, 0.3144679030132573);
    const vec3 c3 = vec3(-27.66873308576866, 14.26473078096533, -13.64921318813922);
    const vec3 c4 = vec3(52.17613981234068, -27.94360607168351, 12.94416944238394);
    const vec3 c5 = vec3(-50.76852536473588, 29.04658282127291, 4.23415299384598);
    const vec3 c6 = vec3(18.65570506591883, -11.48977351997711, -5.601961508734096);
    return c0+t*(c1+t*(c2+t*(c3+t*(c4+t*(c5+t*c6)))));
}

vec3 inferno(float t) {
    const vec3 c0 = vec3(0.0002189403691192265, 0.001651004631001012, -0.01948089843709184);
    const vec3 c1 = vec3(0.1065134194856116, 0.5639564367884091, 3.932712388889277);
    const vec3 c2 = vec3(11.60249308247187, -3.972853965665698, -15.9423941062914);
    const vec3 c3 = vec3(-41.70399613139459, 17.43639888205313, 44.35414519872813);
    const vec3 c4 = vec3(77.162935699427, -33.40235894210092, -81.80730925738993);
    const vec3 c5 = vec3(-71.31942824499214, 32.62606426397723, 73.20951985803202);
    const vec3 c6 = vec3(25.13112622477341, -12.24266895238567, -23.07032500287172);
    return c0+t*(c1+t*(c2+t*(c3+t*(c4+t*(c5+t*c6)))));
}

vec3 gray(float t) {
    const vec3 c0 = vec3(0.0, 0.0, 0.0);
    const vec3 c1 = vec3(1.0, 1.0, 1.0);
    return c0+t*c1;
}

vec3 colormap(int code, float t) {
    if (code == 0) {
        return viridis(t);
    }
    else if (code == 1) {
        return plasma(t);
    }
    else if (code == 2) {
        return magma(t);
    }
    else if (code == 3) {
        return inferno(t);
    }
    else {
        return gray(t);
    }
}

float normalize_value(float value, vec2 bounds) {
    return (value - bounds.x) / (bounds.y - bounds.x);
}

float calculate_shadow(float cos_incidence) {
    if (!shadows) {
        return 0.0;
    }

    vec3 coords_proj = fs_in.pos_lightspace.xyz / fs_in.pos_lightspace.w;
    coords_proj = coords_proj * 0.5 + 0.5;
    
    if (coords_proj.z > 1) {
        return 0.0;
    }
    
    float depth_closest = texture(map_shadow, coords_proj.xy).r;
    float depth_current = coords_proj.z;
    
    float bias = max(bias_acne * (1.0 - cos_incidence), bias_acne);
    float shadow = depth_current - bias > depth_closest ? 1.0 : 0.0;
    
    // PCF
    // float shadow = 0.0;
    // vec2 size_texel = 1.0 / textureSize(map_shadow, 0);
    // for (int x = -1; x <= 1; ++x) {
    //     for (int y = -1; y <= 1; ++y) {
    //         float depth_pcf = texture(map_shadow, coords_proj.xy + vec2(x, y) * size_texel).r;
    //         shadow += depth_current - bias > depth_pcf ? 1.0 : 0.0;
    //     }
    // }
    // shadow /= 9.0;
    
    return shadow;
}

void main() {
    vec3 color = vec3(0.0, 0.0, 0.0);
    
    if (force_color) {
        frag = vec4(forced_color, 1.0);
        return;
    }

    vec3 dir_light = -normalize(pos_light);
    float cos_incidence = max(-dot(dir_light, fs_in.normal), 0.0);
    float shadow = calculate_shadow(cos_incidence);       

    if (fs_in.color_mode == 1) {
        color = fs_in.color;
    }
    else if (fs_in.color_mode == 2) {
        float data_normalized = normalize_value(fs_in.data, colormap_bounds);
        vec3 color_data = colormap(colormap_code, data_normalized);
        color = (1.0 - shadow) * color_data;
    } else {
        // default fs_in.color_mode == 0
        vec3 ambient = ambient_light_color;
        vec3 diffuse = cos_incidence * directional_light_color;
        vec3 color_local = vec3(1.0, 1.0, 1.0) * (1.0 - fs_in.albedo);
        color = (ambient + (1.0 - shadow) * diffuse) * color_local;    
        color = clamp(color, vec3(0.0, 0.0, 0.0), vec3(1.0, 1.0, 1.0));
    }

    frag = vec4(color, 1.0);
}
