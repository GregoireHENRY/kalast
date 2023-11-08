#version 410 core
out uvec3 frag;

uniform uint object_id;
uniform uint draw_id;

void main() {
    frag = uvec3(object_id, draw_id, gl_PrimitiveID);
}
