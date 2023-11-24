use crate::{util::*, Projection, Shapes, Surface};

/// The lighting manager.
///
/// This should be called by a window with OpenGL context so vertices and buffer objects are loaded.
#[derive(Debug, Clone)]
pub struct Light {
    pub(crate) position: Vec3,
    pub(crate) offset: Float,
    pub(crate) projection: Projection,
    pub(crate) cube: Surface,
}

impl Light {
    pub fn new(offset: Float) -> Self {
        let cube = Surface::use_integrated(Shapes::Cube)
            .unwrap()
            /*
            .update_all(|mut v| {
                v.position *= 1e3;
            })
            */
            .build();

        let position = vec3(1.0, 0.0, 0.0).normalize() * offset;
        let side = offset;
        let close = 0.001;
        let far = offset * 2.0;
        let projection = Projection::new_ortho(-side, side, -side, side, close, far);

        Self {
            position,
            offset,
            projection,
            cube,
        }
    }

    #[allow(unused)]
    pub fn direction(&self) -> Vec3 {
        glm::normalize(&(vec3(0.0, 0.0, 0.0) - self.position))
    }

    pub fn set_offset(&mut self, offset: Float) {
        self.offset = offset;
    }

    pub fn set_position(&mut self, pos: &Vec3) {
        self.position = pos.clone();
        self.offset = self.position.magnitude();
        let side = self.offset;
        let close = 0.001;
        let far = self.offset * 2.0;

        self.projection = Projection::new_ortho(-side, side, -side, side, close, far);
    }

    pub fn set_direction(&mut self, dir: &Vec3) {
        self.set_position(&(dir.clone().normalize() * self.offset));
    }
}
