pub mod astro;
pub mod entity;
pub mod gpu;
pub mod app;
pub mod math;
pub mod mesh;
#[cfg(feature = "python")]
pub mod py;
// Python-facing setup types only -- nothing in the engine constructs them,
// and their `#[pyclass]` fields cannot be feature-gated individually because
// the class macro expands before the field attributes do.
#[cfg(feature = "python")]
pub mod routines;
pub mod scattering;
pub mod shadowing;
pub mod spice;
pub mod tpm;
pub mod util;
pub mod meshes;

/// Re-exported for examples, which are compiled against kalast alone when
/// the editor hosts one -- `wgpu::Color` in a config assignment has to be
/// reachable without the example depending on wgpu itself.
pub use wgpu;

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
