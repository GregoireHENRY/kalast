use crate::{
    util::*, FrustrumBase, Projection, ProjectionMode, Shapes, Surface, ASPECT, CLOSE, FAR, SIDE,
};

/// The lighting manager.
///
/// This should be called by a window with OpenGL context so vertices and buffer objects are loaded.
#[derive(Debug, Clone)]
pub struct Light {
    pub position: Vec3,
    pub direction: Vec3,
    pub projection: Projection,
    pub(crate) cube: Surface,
}

impl Light {
    pub fn new(position: Vec3) -> Self {
        let cube = Surface::use_integrated(Shapes::Cube).unwrap().build();

        let distance = position.magnitude();
        let projection = Projection {
            base: FrustrumBase {
                aspect: ASPECT,
                znear: distance * CLOSE,
                zfar: distance * FAR,
            },
            mode: ProjectionMode::Orthographic(distance * SIDE),
        };

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
}
