#version 410 core
layout (location = 0) in vec3 pos;

uniform mat4 matrix_projection;
uniform mat4 matrix_view;
uniform mat4 matrix_model;

void main() {
    gl_Position = matrix_projection * matrix_view * matrix_model * vec4(pos, 1.0);
}
