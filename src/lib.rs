pub mod astro;
pub mod entity;
pub mod gpu;
pub mod app;
pub mod math;
pub mod mesh;
pub mod py;
pub mod routines;
pub mod spice;
pub mod tpm;
pub mod util;
pub mod meshes;

pub type UVec2 = glam::USizeVec2;

#[cfg(feature = "use_f64")]
pub mod float {
    pub type Float = f64;
    pub type Vec2 = glam::DVec2;
    pub type Vec3 = glam::DVec3;
    pub type Vec4 = glam::DVec4;
    pub type Mat3 = glam::DMat3;
    pub type Mat4 = glam::DMat4;
    pub type Quat = glam::DQuat;

    pub use fmod::consts;
    pub use std::f64 as fmod;
}

#[cfg(not(feature = "use_f64"))]
pub mod float {
    pub type Float = f32;
    pub type Vec2 = glam::Vec2;
    pub type Vec3 = glam::Vec3;
    pub type Vec4 = glam::Vec4;
    pub type Mat3 = glam::Mat3;
    pub type Mat4 = glam::Mat4;
    pub type Quat = glam::Quat;

    pub use fmod::consts;
    pub use std::f32 as fmod;
}

pub use float::*;
