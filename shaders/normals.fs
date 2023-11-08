#version 410 core
out vec4 frag;

void main() {
    vec3 color = vec3(0.0, 1.0, 1.0);

    frag = vec4(color, 1.0);
}
