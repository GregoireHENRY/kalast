use crate::{util::*, ProjectionMode, Shapes, Surface, FAR_FACTOR, NEAR_FACTOR};

/// The lighting manager.
///
/// This should be called by a window with OpenGL context so vertices and buffer objects are loaded.
#[derive(Debug, Clone)]
pub struct Light {
    pub position: Vec3,
    pub direction: Vec3,
    pub projection: ProjectionMode,
    pub(crate) cube: Surface,
}

impl Light {
    pub fn new(position: Vec3) -> Self {
        let cube = Surface::use_integrated(Shapes::Cube).unwrap().build();

        Self {
            position,
            direction: Vec3::zeros(),
            projection: ProjectionMode::Orthographic,
            cube,
        }
    }

    pub fn target(&self) -> Vec3 {
        self.position + self.direction
    }

    pub fn matrix_lookat(&self) -> Mat4 {
        glm::look_at(&self.position, &self.target(), &Vec3::z())
    }

    pub fn matrix_projection(&self, aspect: Float) -> Mat4 {
        let distance = self.position.magnitude();
        let near = distance * NEAR_FACTOR;
        let far = distance * FAR_FACTOR;
        self.projection.matrix(near, far, aspect)
    }
}
