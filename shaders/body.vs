#version 410 core
layout (location = 0) in vec3 pos;
layout (location = 1) in vec3 normal;
layout (location = 2) in vec3 color;
layout (location = 3) in float data;
layout (location = 4) in float albedo;
layout (location = 5) in int color_mode;

out VS_OUT {
    vec3 pos;
    vec3 normal;
    vec3 color;
    float data;
    float albedo;
    flat int color_mode;

    vec4 pos_lightspace;
} vs_out;

uniform mat4 matrix_lightspace;
uniform mat4 matrix_projection;
uniform mat4 matrix_view;
uniform mat4 matrix_model;
uniform mat3 matrix_normal;

// Note on model matrix:
// Position coordinates are converted to 4d just the moment of applying matrix 4x4 to include translation and perspective.
// After that, it's always converted back to 3d.

// Note on normal matrix:
// normal vector is normalized after applying normal matrix in case the matrix contains scaling.
// if normal matrix is just rotation, then normalizing is useless.
// translation component is removed since it's a 3x3.

void main() {
    vs_out.pos = vec3(matrix_model * vec4(pos, 1.0));
    vs_out.normal = normalize(matrix_normal * normal);
    vs_out.color = color;
    vs_out.data = data;
    vs_out.albedo = albedo;
    vs_out.color_mode = color_mode;
    vs_out.pos_lightspace = matrix_lightspace * vec4(vs_out.pos, 1.0);

    gl_Position = matrix_projection * matrix_view * vec4(vs_out.pos, 1.0);
}
