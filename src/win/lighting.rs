use crate::{util::*, Projection, Shapes, Surface};

pub const ASPECT_LIGHT: Float = 1.0;

/// The lighting manager.
///
/// This should be called by a window with OpenGL context so vertices and buffer objects are loaded.
#[derive(Debug, Clone)]
pub struct Light {
    pub position: Vec3,
    pub direction: Vec3,
    pub projection: Box<dyn Projection>,
    pub(crate) cube: Surface,
}

impl Light {
    pub fn new(position: Vec3) -> Self {
        let cube = Surface::use_integrated(Shapes::Cube).unwrap().build();
        let mut projection = Projection::new_orthographic();

        Self {
            position,
            direction: Vec3::zeros(),
            projection,
            cube,
        }
    }

    pub fn target(&self) -> Vec3 {
        self.position + self.direction
    }

    pub fn get_look_at_matrix(&self) -> Mat4 {
        glm::look_at(&self.position, &self.target(), &Vec3::z())
    }

    pub fn projection_matrix(&self) -> Mat4 {
        self.projection.build()
    }
}
