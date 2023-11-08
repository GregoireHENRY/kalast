#version 410 core
layout (location = 0) in vec3 position;

uniform mat4 matrix_model_view_projection;

void main() {
    gl_Position = matrix_model_view_projection * vec4(position, 1.0);
}
