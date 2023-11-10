use crate::prelude::*;

use crate::win::camera::*;
use crate::win::graphical_pipeline::VAO;
use crate::win::lighting::*;

#[derive(Debug, Clone)]
pub struct Scene {
    pub(crate) camera: Camera,
    pub(crate) light: Light,
    pub(crate) light_vao: Option<VAO>,
    pub(crate) bodies_vao: Vec<VAO>,
    pub(crate) trajectories_vao: Vec<VAO>,
}

impl Scene {
    pub fn new(settings: &WindowSettings) -> Self {
        let camera = Camera::new(
            vec3(0.0, 0.0, 1.0),
            vec3(0.0, 0.0, 0.0),
            vec3(1.0, 0.0, 0.0),
            settings.camera_speed,
        );
        let light = Light::new(settings.light_offset);

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

    pub fn camera_position(&self) -> &Vec3 {
        &self.camera.position
    }

    pub fn set_camera_position(&mut self, pos: &Vec3) {
        let mut pos = pos.clone();
        if pos.x == 0.0 && pos.y == 0.0 {
            let dx = 1e-5;
            pos = vec3(dx, dx, pos.z);
        }
        self.camera.position = pos;
    }

    pub fn set_light_offset(&mut self, offset: Float) {
        self.light.set_offset(offset);

        if self.light_vao.is_some() {
            self.update_light_vao();
        }
    }

    fn update_light_vao(&mut self) {
        self.light_vao = Some(VAO::smooth_element_buffers(
            &self.light.cube.vertices,
            &self.light.cube.indices,
        ));
    }

    pub fn set_light_position(&mut self, pos: &Vec3) {
        self.light.set_position(pos);

        if self.light_vao.is_some() {
            self.update_light_vao();
        }
    }

    pub fn set_light_direction(&mut self, dir: &Vec3) {
        self.light.set_direction(dir);

        if self.light_vao.is_some() {
            self.update_light_vao();
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
