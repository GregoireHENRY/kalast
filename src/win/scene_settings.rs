use crate::util::*;

/// Settings of scene.
#[derive(Debug, Clone)]
pub struct SceneSettings {
    pub camera_position: Vec3,
    pub light_target: Vec3,
    pub light_target_offset: Float,
    pub light_projection_width: Float,
}

impl Default for SceneSettings {
    fn default() -> Self {
        Self {
            camera_position: vec3(1.0, 0.0, 0.0),
            light_target: Vec3::zeros(),
            light_target_offset: 1.0,
            light_projection_width: 2.0,
        }
    }
}

impl SceneSettings {}
