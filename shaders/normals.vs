#version 410 core
layout (location = 0) in vec3 pos;
layout (location = 1) in vec3 normal;
// layout (location = 2) in vec3 color;
// layout (location = 3) in float data;

out VS_OUT {
    vec3 normal;
} vs_out;

uniform mat4 matrix_view;
uniform mat4 matrix_model;
uniform mat3 matrix_normal_MV;

void main() {
    vs_out.normal = normalize(matrix_normal_MV * normal);

    gl_Position = matrix_view * matrix_model * vec4(pos, 1.0);
}
