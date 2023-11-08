#version 410 core
layout (location = 0) in vec2 pos;
layout (location = 1) in vec2 coords_tex;

out VS_OUT {
    vec2 coords_tex;
} vs_out;

void main() {
    vs_out.coords_tex = coords_tex;
    gl_Position = vec4(pos, 0.0, 1.0);
}
