#version 410 core
out vec4 frag;

uniform vec3 color;

void main() {
    frag = vec4(color, 1.0);
}
