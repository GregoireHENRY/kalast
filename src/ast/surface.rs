use crate::prelude::*;
use crate::python::*;

use crate::ast::element::compute_normal;

use serde::Deserialize;
use serde::Serialize;
use snafu::Location;
use std::fmt::Display;
use std::path::Path;
use tobj::LoadError;

#[allow(unused)]
use std::mem::{size_of, size_of_val};

pub const STR_SHAPE_MODEL_CRATER: &str = include_str!("../../assets/mesh/crater.obj");
pub const STR_SHAPE_MODEL_CUBE: &str = include_str!("../../assets/mesh/cube.obj");
pub const STR_SHAPE_MODEL_ICOSPHERE: &str = include_str!("../../assets/mesh/icosphere.obj");
pub const STR_SHAPE_MODEL_PLANE: &str = include_str!("../../assets/mesh/plane.obj");
pub const STR_SHAPE_MODEL_SPHERE: &str = include_str!("../../assets/mesh/sphere.obj");

pub type Result<T, E = SurfaceError> = std::result::Result<T, E>;

fn compute_raw_facedata(vertices: &Vec<Vertex>, indices: &Vec<u32>) -> Vec<FaceData> {
    if indices.is_empty() {
        vertices
            .chunks_exact(3)
            .map(|vertices_face| {
                FaceData::recompute(&vertices_face[0], &vertices_face[1], &vertices_face[2])
            })
            .collect_vec()
    } else {
        indices
            .chunks_exact(3)
            .map(|indices_face| {
                let i0 = indices_face[0] as usize;
                let i1 = indices_face[1] as usize;
                let i2 = indices_face[2] as usize;

                FaceData::recompute(&vertices[i0], &vertices[i1], &vertices[i2])
            })
            .collect_vec()
    }
}

/// The errors concerning this module.
/// FromPythonError or something
#[derive(Debug, Snafu)]
pub enum SurfaceError {
    FileNotFound {
        source: LoadError,
        path: PathBuf,
        location: Location,
    },
    Unknown {
        source: LoadError,
        location: Location,
    },
}

impl std::convert::From<SurfaceError> for PyErr {
    fn from(err: SurfaceError) -> PyErr {
        PyRuntimeError::new_err(err.to_string())
    }
}

#[pyclass]
#[repr(u8)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum IntegratedShapeModel {
    #[serde(rename = "crater")]
    Crater,

    #[serde(rename = "cube")]
    Cube,

    #[serde(rename = "icosphere")]
    Icosphere,

    #[serde(rename = "plane")]
    Plane,

    #[serde(rename = "sphere")]
    Sphere,
}

impl IntegratedShapeModel {
    pub const fn str(&self) -> &str {
        match self {
            Self::Plane => STR_SHAPE_MODEL_PLANE,
            Self::Cube => STR_SHAPE_MODEL_CUBE,
            Self::Sphere => STR_SHAPE_MODEL_SPHERE,
            Self::Icosphere => STR_SHAPE_MODEL_ICOSPHERE,
            Self::Crater => STR_SHAPE_MODEL_CRATER,
        }
    }
}

/// A raw surface.
#[pyclass(get_all)]
#[derive(Debug, Clone)]
pub struct RawSurface {
    pub positions: Vec<Float>,
    pub indices: Vec<u32>,
}

impl Display for RawSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RawSurface (positions: {}, faces: {})",
            self.positions.len() / 3,
            self.indices.len() / 3
        )
    }
}

impl RawSurface {
    pub fn read_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let (mut models, _) = tobj::load_obj(
            &path,
            &tobj::LoadOptions {
                single_index: true,
                triangulate: true,
                ..Default::default()
            },
        )
        .context(FileNotFoundSnafu { path: &path })?;

        let mut it = models.drain(..);
        let mesh = it.next().unwrap().mesh;

        Ok(Self {
            positions: mesh.positions,
            indices: mesh.indices,
        })
    }

    pub fn use_integrated(model: IntegratedShapeModel) -> Result<Self> {
        let str_shape_model = model.str();
        let mut buf = str_shape_model.as_bytes();

        let (mut models, _) = tobj::load_obj_buf(
            &mut buf,
            &tobj::LoadOptions {
                single_index: true,
                triangulate: true,
                ..Default::default()
            },
            |p| match p.file_name().unwrap().to_str().unwrap() {
                _ => unreachable!(),
            },
        )
        .context(UnknownSnafu {})?;

        let mut it = models.drain(..);
        let mesh = it.next().unwrap().mesh;

        Ok(Self {
            positions: mesh.positions,
            indices: mesh.indices,
        })
    }

    pub fn update_positions(&mut self, closure: fn(Float) -> Float) {
        for pos in &mut self.positions {
            *pos = closure(*pos);
        }
    }

    /*
    pub fn update_all(&mut self, closure: fn(Float) -> Float) {
        let callback = CallbackType::Rust(closure);
        self.__update_all(callback);
    }

    fn __update_all(&mut self, callback: CallbackType<Float>) {
        for value in &mut self.positions {
            *value = callback.call(*value)
        }
    }
    */
}

#[pymethods]
impl RawSurface {
    fn __repr__(&self) -> String {
        format!("{}", self)
    }

    #[classmethod]
    #[pyo3(name = "read_file")]
    #[allow(unused)]
    fn read_file_py(cls: &PyType, path: &str) -> Result<Self> {
        Self::read_file(path)
    }

    /*
    #[classmethod]
    #[pyo3(name = "use_integrated")]
    #[allow(unused)]
    fn use_integrated_py(cls: &PyType, model: IntegratedShapeModel) -> Result<Self> {
        Self::use_integrated(model)
    }

    #[pyo3(name = "update")]
    pub fn update_all_py(&mut self, closure: &PyAny) {
        self.__update_all(CallbackType::Python(closure));
    }
    */
}

/// A surface.
#[pyclass(get_all)]
#[derive(Debug, Clone)]
pub struct Surface {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub faces: Vec<FaceData>,
}

impl Display for Surface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Surface (vertices: {}, faces: {})",
            self.vertices.len(),
            self.indices.len() / 3
        )
    }
}

impl Surface {
    pub fn read_file<P: AsRef<Path>>(path: P) -> Result<SurfaceBuilder> {
        let RawSurface { positions, indices } = RawSurface::read_file(path)?;
        Ok(Self::from(positions, indices))
    }

    pub fn use_integrated(model: IntegratedShapeModel) -> Result<SurfaceBuilder> {
        let RawSurface { positions, indices } = RawSurface::use_integrated(model)?;
        Ok(Self::from(positions, indices))
    }

    pub fn from(positions: Vec<Float>, indices: Vec<u32>) -> SurfaceBuilder {
        let vertices = positions
            .chunks_exact(3)
            .map(|p| Vertex {
                position: vec3(p[0], p[1], p[2]),
                ..Default::default()
            })
            .collect_vec();

        SurfaceBuilder {
            vertices,
            indices,
            do_not_recompute_vertex_normals_at_build: false,
        }
    }

    /// Surface is smooth when indices (that form faces from group of 3 vertices) are provided.
    /// Smooth means working on vertices.
    /// Flat means working on faces.
    pub fn is_smooth(&self) -> bool {
        !self.indices.is_empty()
    }

    pub fn elements(&self) -> Vec<&Vertex> {
        if self.is_smooth() {
            self.vertices.iter().collect_vec()
        } else {
            self.faces.iter().map(|f| &f.vertex).collect_vec()
        }
    }

    pub fn elements_mut(&mut self) -> Vec<&mut Vertex> {
        if self.is_smooth() {
            self.vertices.iter_mut().collect_vec()
        } else {
            self.faces.iter_mut().map(|f| &mut f.vertex).collect_vec()
        }
    }

    pub fn faces_vertices(&self) -> Vec<(&Vertex, &Vertex, &Vertex)> {
        if self.is_smooth() {
            self.indices
                .chunks_exact(3)
                .map(|indices_face| {
                    let i0 = indices_face[0] as usize;
                    let i1 = indices_face[1] as usize;
                    let i2 = indices_face[2] as usize;
                    (&self.vertices[i0], &self.vertices[i1], &self.vertices[i2])
                })
                .collect_vec()
        } else {
            self.vertices
                .chunks_exact(3)
                .map(|c| (&c[0], &c[1], &c[2]))
                .collect_vec()
        }
    }

    pub fn apply_facedata_to_vertices(&mut self) {
        if self.is_smooth() {
            return;
        }

        for (face, face_vertices) in izip!(&self.faces, self.vertices.chunks_exact_mut(3)) {
            face_vertices[0].color = face.vertex.color;
            face_vertices[1].color = face.vertex.color;
            face_vertices[2].color = face.vertex.color;

            face_vertices[0].data = face.vertex.data;
            face_vertices[1].data = face.vertex.data;
            face_vertices[2].data = face.vertex.data;

            face_vertices[0].material = face.vertex.material;
            face_vertices[1].material = face.vertex.material;
            face_vertices[2].material = face.vertex.material;

            face_vertices[0].color_mode = face.vertex.color_mode;
            face_vertices[1].color_mode = face.vertex.color_mode;
            face_vertices[2].color_mode = face.vertex.color_mode;
        }
    }

    pub fn update_vertices(&mut self, closure: fn(&mut Vertex)) {
        for vertex in &mut self.vertices {
            closure(vertex);
        }

        for face in &mut self.faces {
            closure(&mut face.vertex);
        }
    }
}

/// Builder of surface.
#[pyclass(get_all)]
#[derive(Debug, Clone)]
pub struct SurfaceBuilder {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,

    pub do_not_recompute_vertex_normals_at_build: bool,
}

impl Display for SurfaceBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SurfaceBuilder (vertices: {}, faces: {})",
            self.vertices.len(),
            self.indices.len() / 3
        )
    }
}

impl SurfaceBuilder {
    pub fn flat(mut self) -> Self {
        if !self.indices.is_empty() {
            self.vertices = self
                .indices
                .chunks_exact(3)
                .map(|indices_face| {
                    let i0 = indices_face[0] as usize;
                    let i1 = indices_face[1] as usize;
                    let i2 = indices_face[2] as usize;

                    [self.vertices[i0], self.vertices[i1], self.vertices[i2]]
                })
                .flatten()
                .collect_vec();
            self.indices.clear();
        }

        self
    }

    #[allow(unreachable_code)]
    pub fn smooth(self) -> Self {
        unimplemented!();

        if self.indices.is_empty() {
            // TODO:
            //   1) read vertices by chunk of 3 to get vertex indices that form faces, but get only indices of all unique vertices -> you will create the list of indices
            //   2) remove all redundent vertices -> you will create the new list of vertices (less than before)
        }

        self
    }

    pub fn update_all<F>(mut self, callback: F) -> Self
    where
        F: Fn(&mut Vertex),
    {
        for vertex in &mut self.vertices {
            callback(vertex);
        }
        self
    }

    /*
    pub fn update_all<F>(mut self, closure: F) -> Self
    where
        F: Fn(Vertex) -> Vertex,
    {
        let callback = CallbackType::Rust(closure);
        self.update_all_internal(callback);
        self
    }

    fn __update_all(&mut self, callback: CallbackType<Vertex>) -> &mut Self {
        self.update_all_internal(callback);
        self
    }

    fn update_all_internal<F>(&mut self, callback: CallbackType<F>)
    where
        F: Fn(Vertex) -> Vertex,
    {
        for vertex in &mut self.vertices {
            *vertex = callback.call(*vertex);
        }
    }
    */

    pub fn build(self) -> Surface {
        let Self {
            mut vertices,
            indices,
            do_not_recompute_vertex_normals_at_build,
        } = self.clone();

        let faces = compute_raw_facedata(&vertices, &indices);

        if !do_not_recompute_vertex_normals_at_build {
            if indices.is_empty() {
                for (vertices_face, face) in izip!(vertices.chunks_exact_mut(3), &faces) {
                    vertices_face[0].normal = face.vertex.normal;
                    vertices_face[1].normal = face.vertex.normal;
                    vertices_face[2].normal = face.vertex.normal;
                }
            } else {
                for vertex in &mut vertices {
                    vertex.normal = vec3(0.0, 0.0, 0.0);
                }

                for indices_face in indices.chunks_exact(3) {
                    let i0 = indices_face[0] as usize;
                    let i1 = indices_face[1] as usize;
                    let i2 = indices_face[2] as usize;

                    let p0 = vertices[i0].position;
                    let p1 = vertices[i1].position;
                    let p2 = vertices[i2].position;

                    let normal = compute_normal(&p0, &p1, &p2);
                    vertices[i0].normal += normal;
                    vertices[i1].normal += normal;
                    vertices[i2].normal += normal;
                }

                for vertex in &mut vertices {
                    vertex.normal = glm::normalize(&vertex.normal);
                }
            }
        }

        Surface {
            vertices,
            indices,
            faces,
        }
    }
}
