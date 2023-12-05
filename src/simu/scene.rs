use crate::util::*;

#[derive(Debug, Clone)]
pub struct Scene {
    pub camera: Vec3,
    pub sun: Vec3,
}

impl Scene {
    pub fn sun_dir(&self) -> Vec3 {
        self.sun.normalize()
    }

    pub fn sun_dist(&self) -> Float {
        self.sun.magnitude()
    }
    pub fn cam_dir(&self) -> Vec3 {
        self.camera.normalize()
    }

    pub fn cam_dist(&self) -> Float {
        self.camera.magnitude()
    }

    pub fn sun_pos_cubelight(&self) -> Vec3 {
        self.sun_dir() * self.cam_dist()
    }
}
