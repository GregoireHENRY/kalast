#version 410 core
layout (location = 0) in vec3 position;

uniform mat4 matrix_lightspace;
uniform mat4 matrix_model;
uniform mat3 matrix_normal; // unused

void main()
{
    gl_Position = matrix_lightspace * matrix_model * vec4(position, 1.0);
}