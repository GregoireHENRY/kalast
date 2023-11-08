#version 410 core
out vec4 frag;

in VS_OUT {
    vec2 coords_tex;
} fs_in;

uniform sampler2D map_depth;

void main() {
    /*
    vec3 color = texture(map_depth, fs_in.coords_tex).rgb;
    fragment = vec4(color, 1.0);
    // float average = (0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b) / 3.0;
    // fragment = vec4(vec3(average), 1.0);
    */

    float depth = texture(map_depth, fs_in.coords_tex).r;
    frag = vec4(vec3(depth), 1.0);
}

