use crate::prelude::*;
use crate::python::*;

use std::fmt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ColorMode {
    #[serde(rename = "diffuse_light")]
    DiffuseLight,

    #[serde(rename = "color")]
    Color,
    
    #[serde(rename = "data")]
    Data,
}

impl Default for ColorMode {
    fn default() -> Self {
        Self::DiffuseLight
    }
}

fn averaged_material(a: &Material, b: &Material, c: &Material) -> Material {
    let albedo = (a.albedo + b.albedo + c.albedo) / 3.0;
    let emissivity = (a.emissivity + b.emissivity + c.emissivity) / 3.0;
    let thermal_inertia = (a.thermal_inertia + b.thermal_inertia + c.thermal_inertia) / 3.0;
    let density = (a.density + b.density + c.density) / 3.0;
    let heat_capacity = (a.heat_capacity + b.heat_capacity + c.heat_capacity) / 3.0;
    Material {
        albedo,
        emissivity,
        thermal_inertia,
        density,
        heat_capacity,
    }
}

pub(crate) fn compute_normal(a: &Vec3, b: &Vec3, c: &Vec3) -> Vec3 {
    glm::normalize(&(b - a).cross(&(c - a)))
}

pub(crate) fn compute_area(a: &Vec3, b: &Vec3, c: &Vec3) -> Float {
    0.5 * (b - a).angle(&(c - a)).sin() * (b - a).magnitude() * (c - a).magnitude()
}

/// A vertex.
#[pyclass]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub color: Vec3,
    pub data: Float,
    pub material: Material,
    pub color_mode: ColorMode,
}

impl Default for Vertex {
    fn default() -> Self {
        Self {
            position: Vec3::zeros(),
            normal: Vec3::zeros(),
            color: vec3(1.0, 1.0, 1.0),
            data: 0.0,
            material: Material {
                albedo: 0.0,
                emissivity: 1.0,
                thermal_inertia: 0.0,
                density: 0.0,
                heat_capacity: 0.0,
            },
            color_mode: ColorMode::default(),
        }
    }
}

impl fmt::Display for Vertex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "- vertex:\n- position: {:?}\n- normal: {:?}\n- color: {:?}\n- data: {}\n- {:?}\n- color mode: {:?}\n- spherical coordinates: {:?}",
            self.position.as_slice(),
            self.normal.as_slice(),
            self.color.as_slice(),
            self.data,
            self.material,
            self.color_mode,
            self.sph().as_slice(),
        )
    }
}

impl Vertex {
    pub fn sph(&self) -> Vec3 {
        util::cartesian_to_spherical(&self.position)
    }
}

#[pymethods]
impl Vertex {
    fn __repr__(&self) -> String {
        format!("{:?}", self)
    }

    #[getter]
    fn get_data(&self) -> Float {
        self.data
    }

    #[getter]
    fn get_material(&self, py: Python) -> PyObject {
        self.material.into_py(py)
    }

    #[setter]
    fn set_data(&mut self, data: Float) {
        self.data = data;
    }

    #[setter]
    fn set_material(&mut self, material: Material) {
        self.material = material;
    }
}

/// Data of a face.
#[pyclass]
#[derive(Debug, Clone, Copy)]
pub struct FaceData {
    pub vertex: Vertex, // virtual vertex at the center of the facet.
    pub area: Float,
}

impl fmt::Display for FaceData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Face:\n{}\n- area: {}",
            util::fmt_str_tab(&format!("{}", self.vertex), 1),
            self.area
        )
    }
}

impl FaceData {
    pub fn recompute(a: &Vertex, b: &Vertex, c: &Vertex) -> Self {
        let position = (a.position + b.position + c.position) / 3.0;
        let normal = compute_normal(&a.position, &b.position, &c.position);
        let color = (a.color + b.color + c.color) / 3.0;
        let data = (a.data + b.data + c.data) / 3.0;
        let color_mode = a.color_mode;
        let material = averaged_material(&a.material, &b.material, &c.material);
        let area = compute_area(&a.position, &b.position, &c.position);
        let vertex = Vertex {
            position,
            normal,
            color,
            data,
            material,
            color_mode,
        };
        Self { vertex, area }
    }

    pub fn average(a: &Vertex, b: &Vertex, c: &Vertex) -> Self {
        let position = (a.position + b.position + c.position) / 3.0;
        let normal = (a.normal + b.normal + c.normal) / 3.0;
        let color = (a.color + b.color + c.color) / 3.0;
        let data = (a.data + b.data + c.data) / 3.0;
        let color_mode = a.color_mode;
        let material = averaged_material(&a.material, &b.material, &c.material);
        let area = compute_area(&a.position, &b.position, &c.position);
        let vertex = Vertex {
            position,
            normal,
            color,
            data,
            material,
            color_mode,
        };
        Self { vertex, area }
    }
}

#[pymethods]
impl FaceData {
    #[classmethod]
    #[pyo3(name = "recompute")]
    #[allow(unused)]
    fn recompute_py(cls: &PyType, a: &Vertex, b: &Vertex, c: &Vertex) -> Self {
        Self::recompute(a, b, c)
    }

    #[classmethod]
    #[pyo3(name = "average")]
    #[allow(unused)]
    fn average_py(cls: &PyType, a: &Vertex, b: &Vertex, c: &Vertex) -> Self {
        Self::average(a, b, c)
    }

    #[getter]
    pub fn get_vertex(&self) -> Vertex {
        self.vertex
    }

    #[getter]
    pub fn get_area(&self) -> Float {
        self.area
    }

    #[setter]
    pub fn set_vertex(&mut self, vertex: Vertex) {
        self.vertex = vertex;
    }

    #[setter]
    pub fn set_area(&mut self, area: Float) {
        self.area = area;
    }
}
