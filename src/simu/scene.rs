use crate::prelude::*;

pub struct Scene {
    pub sun_pos: Vec3,
    pub cam_pos: Vec3,
}

impl Scene {
    pub fn sun_dir(&self) -> Vec3 {
        self.sun_pos.normalize()
    }

    pub fn sun_dist(&self) -> Float {
        self.sun_pos.magnitude()
    }
    pub fn cam_dir(&self) -> Vec3 {
        self.cam_pos.normalize()
    }

    pub fn cam_dist(&self) -> Float {
        self.cam_pos.magnitude()
    }

    pub fn sun_pos_cubelight(&self) -> Vec3 {
        self.sun_dir() * self.cam_dist()
    }
}
