use crate::prelude::*;
use crate::python::*;

/// Settings of scene.
#[pyclass]
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

#[pymethods]
impl SceneSettings {
    #[new]
    fn new_py() -> Self {
        Self::default()
    }

    #[classmethod]
    #[pyo3(name = "default")]
    #[allow(unused)]
    fn default_py(cls: &PyType) -> Self {
        Self::default()
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self)
    }

    #[getter]
    fn get_light_target_offset(&self) -> Float {
        self.light_target_offset
    }

    #[getter]
    fn get_light_projection_width(&self) -> Float {
        self.light_projection_width
    }

    #[setter]
    fn set_light_target_offset(&mut self, offset: Float) {
        self.light_target_offset = offset;
    }

    #[setter]
    fn set_light_projection_width(&mut self, width: Float) {
        self.light_projection_width = width;
    }
}
