#version 410 core
layout (triangles) in;
layout (line_strip, max_vertices = 6) out;

in VS_OUT {
    vec3 normal;
} gs_in[];

uniform mat4 matrix_projection;
uniform float magnitude;

void generate_line(int index)
{
    gl_Position = matrix_projection * gl_in[index].gl_Position;
    EmitVertex();
    gl_Position = matrix_projection * (gl_in[index].gl_Position + vec4(gs_in[index].normal, 0.0) * magnitude);
    EmitVertex();
    EndPrimitive();
}

void generate_line_average(int i0, int i1, int i2)
{
    vec4 center = (gl_in[i0].gl_Position + gl_in[i1].gl_Position + gl_in[i2].gl_Position) / 3;
    vec3 normal = (gs_in[i0].normal + gs_in[i1].normal + gs_in[i2].normal) / 3;
    gl_Position = matrix_projection * center;
    EmitVertex();
    gl_Position = matrix_projection * (center + vec4(normal, 0.0) * magnitude);
    EmitVertex();
    EndPrimitive();
}

void main()
{
    /*
    generate_line(0); // first vertex normal
    generate_line(1); // second vertex normal
    generate_line(2); // third vertex normal
    */

    generate_line_average(0, 1, 2);
}