use crate::{util::*, Camera, Light, ProjectionMode, Surface, WindowSettings, VAO};

use itertools::izip;

#[derive(Debug, Clone)]
pub struct WindowScene {
    pub camera: Camera,
    pub light: Light,
    pub(crate) light_vao: Option<VAO>,
    pub(crate) bodies_vao: Vec<VAO>,
    pub(crate) trajectories_vao: Vec<VAO>,
}

impl WindowScene {
    pub fn new(settings: &WindowSettings) -> Self {
        let mut camera = Camera::new(
            settings.camera_up,
            settings.camera_direction,
            settings.camera_position,
        );

        if settings.ortho {
            camera.projection.mode = ProjectionMode::Orthographic(settings.fovy);
        } else {
            camera.projection.mode = ProjectionMode::Perspective(camera.position.magnitude());
        }

        let light = Light::new(settings.light_position);

        let light_vao = settings.show_light.then_some(VAO::smooth_element_buffers(
            &light.cube.vertices,
            &light.cube.indices,
        ));

        Self {
            camera,
            light,
            light_vao,
            bodies_vao: vec![],
            trajectories_vao: vec![],
        }
    }

    pub(crate) fn compute_asteroid_vao(&self, surf: &Surface) -> VAO {
        if surf.indices.is_empty() {
            VAO::flat_vertices_buffers(&surf.vertices)
        } else {
            VAO::smooth_element_buffers(&surf.vertices, &surf.indices)
        }
    }

    pub fn load_surfaces<'a, I>(&mut self, surfaces: I)
    where
        I: IntoIterator<Item = &'a Surface>,
    {
        for surf in surfaces {
            self.load_surface(surf);
        }
    }

    pub fn load_surface(&mut self, surf: &Surface) {
        self.bodies_vao.push(self.compute_asteroid_vao(surf));
    }

    pub fn load_trajectory(&mut self, points: &[Vec3]) {
        self.trajectories_vao.push(VAO::quick_vec3_buffers(points));
    }

    pub fn update_surface_data(&mut self, vao_index: usize, surf: &mut Surface) {
        let vao = &self.bodies_vao[vao_index];

        if surf.indices.is_empty() {
            for (face_vertices, &face) in izip!(surf.vertices.chunks_exact_mut(3), &surf.faces) {
                face_vertices[0].data = face.vertex.data;
                face_vertices[1].data = face.vertex.data;
                face_vertices[2].data = face.vertex.data;
            }
        }

        vao.update_vertex_buffer(&surf.vertices);
    }
}
