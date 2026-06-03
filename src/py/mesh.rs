use std::{cell::RefCell, rc::Rc};

use image::GenericImageView;
use numpy::ToPyArray;
use pyo3::prelude::*;

use crate::{Float, Vec3};

#[pyclass(from_py_object, unsendable)]
#[derive(Clone)]
pub struct Vertex {
    pub inner: Rc<RefCell<crate::mesh::Vertex>>,
}

#[pymethods]
impl Vertex {
    #[new]
    #[pyo3(signature = (
        pos=None,
        tex=None,
        normal=None,
        tangent=None,
        bitangent=None,
        color=None,
        color_mode=None,
    ))]
    pub fn new(
        pos: Option<[Float; 3]>,
        tex: Option<[Float; 2]>,
        normal: Option<[Float; 3]>,
        tangent: Option<[Float; 3]>,
        bitangent: Option<[Float; 3]>,
        color: Option<[Float; 3]>,
        color_mode: Option<u32>,
    ) -> Self {
        let mut vertex = crate::mesh::Vertex::default();

        if let Some(pos) = pos {
            vertex.pos = pos.into();
        }
        if let Some(tex) = tex {
            vertex.tex = tex.into();
        }
        if let Some(normal) = normal {
            vertex.normal = normal.into();
        }
        if let Some(tangent) = tangent {
            vertex.tangent = tangent.into();
        }
        if let Some(bitangent) = bitangent {
            vertex.bitangent = bitangent.into();
        }
        if let Some(color) = color {
            vertex.color = color.into();
        }
        if let Some(color_mode) = color_mode {
            vertex.color_mode = color_mode.into();
        }

        Self {
            inner: Rc::new(RefCell::new(vertex)),
        }
    }

    #[getter]
    fn pos<'py>(slf: pyo3::Bound<'py, Self>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let inner = &slf.borrow().inner;
        let slice = &inner.borrow().pos;
        let arr = ndarray::ArrayView1::from(slice.as_ref());
        unsafe { numpy::PyArray1::borrow_from_array(&arr, slf.into_any()) }
    }

    #[setter]
    fn set_pos(&self, arr: [Float; 3]) {
        self.inner.borrow_mut().pos = arr.into();
    }

    #[getter]
    fn tex<'py>(slf: pyo3::Bound<'py, Self>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let inner = &slf.borrow().inner;
        let slice = &inner.borrow().tex;
        let arr = ndarray::ArrayView1::from(slice.as_ref());
        unsafe { numpy::PyArray1::borrow_from_array(&arr, slf.into_any()) }
    }

    #[setter]
    fn set_tex(&self, arr: [Float; 2]) {
        self.inner.borrow_mut().tex = arr.into();
    }

    #[getter]
    fn normal<'py>(slf: pyo3::Bound<'py, Self>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let inner = &slf.borrow().inner;
        let slice = &inner.borrow().normal;
        let arr = ndarray::ArrayView1::from(slice.as_ref());
        unsafe { numpy::PyArray1::borrow_from_array(&arr, slf.into_any()) }
    }

    #[setter]
    fn set_normal(&self, arr: [Float; 3]) {
        self.inner.borrow_mut().normal = arr.into();
    }

    #[getter]
    fn tangent<'py>(slf: pyo3::Bound<'py, Self>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let inner = &slf.borrow().inner;
        let slice = &inner.borrow().tangent;
        let arr = ndarray::ArrayView1::from(slice.as_ref());
        unsafe { numpy::PyArray1::borrow_from_array(&arr, slf.into_any()) }
    }

    #[setter]
    fn set_tangent(&self, arr: [Float; 3]) {
        self.inner.borrow_mut().tangent = arr.into();
    }

    #[getter]
    fn bitangent<'py>(slf: pyo3::Bound<'py, Self>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let inner = &slf.borrow().inner;
        let slice = &inner.borrow().bitangent;
        let arr = ndarray::ArrayView1::from(slice.as_ref());
        unsafe { numpy::PyArray1::borrow_from_array(&arr, slf.into_any()) }
    }

    #[setter]
    fn set_bitangent(&self, arr: [Float; 3]) {
        self.inner.borrow_mut().bitangent = arr.into();
    }

    #[getter]
    fn color<'py>(slf: pyo3::Bound<'py, Self>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let inner = &slf.borrow().inner;
        let slice = &inner.borrow().color;
        let arr = ndarray::ArrayView1::from(slice.as_ref());
        unsafe { numpy::PyArray1::borrow_from_array(&arr, slf.into_any()) }
    }

    #[setter]
    fn set_color(&self, arr: [Float; 3]) {
        self.inner.borrow_mut().color = arr.into();
    }

    #[getter]
    fn color_mode(&self) -> u32 {
        self.inner.borrow().color_mode
    }

    #[setter]
    fn set_color_mode(&self, mode: u32) {
        self.inner.borrow_mut().color_mode = mode;
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.inner.borrow())
    }
}

impl std::fmt::Debug for Vertex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.inner.borrow())
    }
}

#[pyclass(from_py_object, unsendable)]
#[derive(Clone)]
pub struct Facet {
    pub inner: Rc<RefCell<crate::mesh::Facet>>,
}

#[pymethods]
impl Facet {
    #[new]
    #[pyo3(signature = (
        pos=None,
        normal=None,
        area=None,
    ))]
    pub fn new(pos: Option<[Float; 3]>, normal: Option<[Float; 3]>, area: Option<Float>) -> Self {
        let mut facet = crate::mesh::Facet::default();

        if let Some(pos) = pos {
            facet.pos = pos.into();
        }
        if let Some(normal) = normal {
            facet.normal = normal.into();
        }
        if let Some(area) = area {
            facet.area = area.into();
        }

        Self {
            inner: Rc::new(RefCell::new(facet)),
        }
    }

    #[getter]
    fn pos<'py>(slf: pyo3::Bound<'py, Self>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let inner = &slf.borrow().inner;
        let slice = &inner.borrow().pos;
        let arr = ndarray::ArrayView1::from(slice.as_ref());
        unsafe { numpy::PyArray1::borrow_from_array(&arr, slf.into_any()) }
    }

    #[setter]
    fn set_pos(&self, arr: [Float; 3]) {
        self.inner.borrow_mut().pos = arr.into();
    }

    #[getter]
    fn normal<'py>(slf: pyo3::Bound<'py, Self>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let inner = &slf.borrow().inner;
        let slice = &inner.borrow().normal;
        let arr = ndarray::ArrayView1::from(slice.as_ref());
        unsafe { numpy::PyArray1::borrow_from_array(&arr, slf.into_any()) }
    }

    #[setter]
    fn set_normal(&self, arr: [Float; 3]) {
        self.inner.borrow_mut().normal = arr.into();
    }

    #[getter]
    fn area(&self) -> Float {
        self.inner.borrow().area
    }

    #[setter]
    fn set_area(&self, area: Float) {
        self.inner.borrow_mut().area = area;
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.inner.borrow())
    }
}

impl std::fmt::Debug for Facet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.inner.borrow())
    }
}

#[pyclass(from_py_object, unsendable)]
#[derive(Clone)]
pub struct Material {
    pub inner: Rc<RefCell<crate::mesh::Material>>,
}

#[pymethods]
impl Material {
    pub fn diffuse_bytes(&self) -> PyResult<Vec<u8>> {
        Ok(self.inner.borrow().diffuse.clone().into_bytes())
    }

    pub fn diffuse_dimensions(&self) -> PyResult<(u32, u32)> {
        Ok(self.inner.borrow().diffuse.dimensions())
    }

    pub fn normal_bytes(&self) -> PyResult<Vec<u8>> {
        Ok(self.inner.borrow().normal.clone().into_bytes())
    }

    pub fn normal_dimensions(&self) -> PyResult<(u32, u32)> {
        Ok(self.inner.borrow().normal.dimensions())
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.inner.borrow())
    }
}

impl std::fmt::Debug for Material {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.inner.borrow())
    }
}

#[pyclass(from_py_object, unsendable)]
#[derive(Clone)]
pub struct Mesh {
    pub inner: Rc<RefCell<crate::mesh::Mesh>>,
}

impl Mesh {
    fn _load(path: &str, update_pos: Option<Bound<'_, PyAny>>) -> Self {
        let update_pos = |v: Vec3| {
            Python::attach(|py| {
                if let Some(update_pos) = update_pos.as_ref() {
                    update_pos
                        .call1((v.to_array().to_pyarray(py),))
                        .unwrap()
                        .extract::<[Float; 3]>()
                        .unwrap()
                        .into()
                } else {
                    v
                }
            })
        };

        let mesh = crate::mesh::Mesh::load(path, update_pos);

        Self {
            inner: Rc::new(RefCell::new(mesh)),
        }
    }
}

#[pymethods]
impl Mesh {
    #[new]
    #[pyo3(signature = (
        path=None,
        update_pos=None,
        vertices=None,
        facets=None,
        indices=None,
        material_id=None,
    ))]
    pub fn new(
        path: Option<&str>,
        update_pos: Option<Bound<'_, PyAny>>,
        vertices: Option<Vec<Vertex>>,
        facets: Option<Vec<Facet>>,
        indices: Option<Vec<u32>>,
        material_id: Option<usize>,
    ) -> Self {
        if let Some(path) = path {
            Self::_load(path, update_pos)
        } else {
            let mut mesh = crate::mesh::Mesh::new();

            if let Some(vs) = vertices {
                mesh.vertices = vs.into_iter().map(|v| v.inner.borrow().clone()).collect();
            }

            if let Some(fs) = facets {
                mesh.facets = fs.into_iter().map(|f| f.inner.borrow().clone()).collect();
            }

            if let Some(idx) = indices {
                mesh.indices = idx;
            }

            if let Some(id) = material_id {
                mesh.material_id = Some(id);
            }

            Self {
                inner: Rc::new(RefCell::new(mesh)),
            }
        }
    }

    #[classmethod]
    #[pyo3(signature = (path, update_pos=None))]
    pub fn load(
        _cls: Bound<'_, pyo3::types::PyType>,
        path: &str,
        update_pos: Option<Bound<'_, PyAny>>,
    ) -> Self {
        Self::_load(path, update_pos)
    }

    #[getter]
    fn vertices(&self) -> VerticesView {
        VerticesView {
            mesh: self.inner.clone(),
        }
    }

    #[getter]
    fn indices(slf: Bound<'_, Self>) -> Bound<'_, numpy::PyArray1<u32>> {
        let inner = &slf.borrow().inner;
        let slice = &inner.borrow().indices;
        let arr = ndarray::ArrayView1::from(slice);
        unsafe { numpy::PyArray1::borrow_from_array(&arr, slf.into_any()) }
    }

    #[getter]
    fn facets(&self) -> FacetsView {
        FacetsView {
            mesh: self.inner.clone(),
        }
    }

    #[getter]
    fn material_id(&self) -> Option<usize> {
        self.inner.borrow().material_id
    }

    #[setter]
    fn set_material_id(&mut self, id: Option<usize>) {
        self.inner.borrow_mut().material_id = id;
    }

    #[getter]
    fn _vertices_before_flatten(&self) -> VerticesBeforeFlattenView {
        VerticesBeforeFlattenView {
            mesh: self.inner.clone(),
        }
    }

    #[getter]
    fn positions(slf: Bound<'_, Self>) -> Bound<'_, numpy::PyArray2<Float>> {
        // println!("{}", std::mem::size_of::<crate::mesh::Vertex>());
        vertex_matrix_array(slf, crate::mesh::POS_OFFSET, 3)
    }

    #[getter]
    fn textures(slf: Bound<'_, Self>) -> Bound<'_, numpy::PyArray2<Float>> {
        vertex_matrix_array(slf, crate::mesh::TEX_OFFSET, 2)
    }

    #[getter]
    fn normals(slf: Bound<'_, Self>) -> Bound<'_, numpy::PyArray2<Float>> {
        vertex_matrix_array(slf, crate::mesh::NORMAL_OFFSET, 3)
    }

    #[getter]
    fn tangents(slf: Bound<'_, Self>) -> Bound<'_, numpy::PyArray2<Float>> {
        vertex_matrix_array(slf, crate::mesh::TANGENT_OFFSET, 3)
    }

    #[getter]
    fn bitangents(slf: Bound<'_, Self>) -> Bound<'_, numpy::PyArray2<Float>> {
        vertex_matrix_array(slf, crate::mesh::BITANGENT_OFFSET, 3)
    }

    #[getter]
    fn colors(slf: Bound<'_, Self>) -> Bound<'_, numpy::PyArray2<Float>> {
        vertex_matrix_array(slf, crate::mesh::COLOR_OFFSET, 3)
    }

    #[getter]
    fn color_modes(slf: Bound<'_, Self>) -> Bound<'_, numpy::PyArray1<u32>> {
        let start = crate::mesh::COLOR_MODE_OFFSET;
        let stride = crate::mesh::VERTEX_STRIDE;

        #[cfg(feature = "use_f64")]
        let start = start * 2;

        #[cfg(feature = "use_f64")]
        let stride = stride * 2;

        let size = 1;

        let mesh = slf.borrow();
        let mesh = mesh.inner.borrow();

        let slice: &[u32] = bytemuck::cast_slice(&mesh.vertices);
        let arr = ndarray::ArrayView1::from(slice)
            .into_shape_with_order((mesh.vertices.len(), stride))
            .unwrap();

        let arr = arr.slice(ndarray::s![.., start..start + size]);
        let arr = arr.flatten();
        unsafe { numpy::PyArray1::borrow_from_array(&arr, slf.into_any()) }
    }

    fn flatten(&mut self) {
        self.inner.borrow_mut().flatten();
    }

    fn smoothen(&mut self) {
        self.inner.borrow_mut().smoothen();
    }

    fn is_flat(&self) -> bool {
        self.inner.borrow().is_flat()
    }

    fn get_facet_vertices(&self, facet: isize) -> FacetVerticesView {
        let n = self.inner.borrow().facets.len();
        let facet = super::util::isize_to_usize(facet, n).unwrap();
        FacetVerticesView {
            mesh: self.inner.clone(),
            index: facet,
        }
    }

    fn get_facet_indices(&self, facet: isize) -> [u32; 3] {
        let n = self.inner.borrow().facets.len();
        let facet = super::util::isize_to_usize(facet, n).unwrap();
        self.inner.borrow().get_facet_indices(facet)
    }

    fn get_facet_positions(
        slf: Bound<'_, Self>,
        facet: isize,
    ) -> [Bound<'_, numpy::PyArray1<Float>>; 3] {
        let n = slf.borrow().inner.borrow().facets.len();
        let facet = super::util::isize_to_usize(facet, n).unwrap();
        facets_matrix_array(slf, crate::mesh::POS_OFFSET, 3, facet)
    }

    fn get_facet_normals(
        slf: Bound<'_, Self>,
        facet: isize,
    ) -> [Bound<'_, numpy::PyArray1<Float>>; 3] {
        let n = slf.borrow().inner.borrow().facets.len();
        let facet = super::util::isize_to_usize(facet, n).unwrap();
        facets_matrix_array(slf, crate::mesh::NORMAL_OFFSET, 3, facet)
    }

    fn get_facet_colors(
        slf: Bound<'_, Self>,
        facet: isize,
    ) -> [Bound<'_, numpy::PyArray1<Float>>; 3] {
        let n = slf.borrow().inner.borrow().facets.len();
        let facet = super::util::isize_to_usize(facet, n).unwrap();
        facets_matrix_array(slf, crate::mesh::COLOR_OFFSET, 3, facet)
    }

    fn update_all_vertices_colors(&mut self, mode: u32, color: [Float; 3]) {
        self.inner
            .borrow_mut()
            .update_all_vertices_colors(mode, color.into());
    }

    #[pyo3(signature = (p, u, exit_first=false))]
    fn intersect(
        &self,
        p: [Float; 3],
        u: [Float; 3],
        exit_first: bool,
    ) -> Option<(usize, [Float; 3])> {
        self.inner
            .borrow()
            .intersect(&p.into(), &u.into(), exit_first)
            .map(|(i, x)| (i, x.into()))
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.inner.borrow())
    }
}

crate::impl_mesh_view!(VerticesView, VertexView, Vertex, vertices);
crate::impl_mesh_view!(
    VerticesBeforeFlattenView,
    VertexBeforeFlattenView,
    Vertex,
    _vertices_before_flatten
);
crate::impl_mesh_view!(FacetsView, FacetView, Facet, facets);

crate::impl_mesh_field_vec!(VertexView, vertices, pos);
crate::impl_mesh_field_vec!(VertexView, vertices, tex);
crate::impl_mesh_field_vec!(VertexView, vertices, normal);
crate::impl_mesh_field_vec!(VertexView, vertices, tangent);
crate::impl_mesh_field_vec!(VertexView, vertices, bitangent);
crate::impl_mesh_field_vec!(VertexView, vertices, color);
crate::impl_mesh_field_scalar!(VertexView, vertices, color_mode, u32);

crate::impl_mesh_field_vec!(FacetView, facets, pos);
crate::impl_mesh_field_vec!(FacetView, facets, normal);

#[pyclass(unsendable)]
pub struct FacetVerticesView {
    pub mesh: Rc<RefCell<crate::mesh::Mesh>>,
    pub index: usize,
}

#[pymethods]
impl FacetVerticesView {
    fn __len__(&self) -> usize {
        self.mesh.borrow().facets.len()
    }

    fn __getitem__(&self, index: isize) -> PyResult<VertexView> {
        let indices = self.mesh.borrow().get_facet_indices(self.index);
        let index = super::util::isize_to_usize(index, 3)?;

        Ok(VertexView {
            mesh: self.mesh.clone(),
            index: indices[index] as usize,
        })
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.mesh.borrow().get_facet_vertices(self.index))
    }
}

fn vertex_matrix<'a>(mesh: &'a crate::mesh::Mesh) -> ndarray::ArrayView2<'a, Float> {
    let slice: &[Float] = bytemuck::cast_slice(&mesh.vertices);
    // println!("{}", slice.len());
    // println!("{} {}", mesh.vertices.len(), mesh.facets.len());

    ndarray::ArrayView1::from(slice)
        .into_shape_with_order((mesh.vertices.len(), crate::mesh::VERTEX_STRIDE))
        .unwrap()
}

fn vertex_matrix_array<'a>(
    slf: Bound<'_, Mesh>,
    start: usize,
    size: usize,
) -> Bound<'_, numpy::PyArray2<Float>> {
    let mesh = slf.borrow();
    let mesh = mesh.inner.borrow();
    let arr = vertex_matrix(&mesh);
    let arr = arr.slice(ndarray::s![.., start..start + size]);
    unsafe { numpy::PyArray2::borrow_from_array(&arr, slf.into_any()) }
}

fn facets_matrix_array<'a>(
    slf: Bound<'_, Mesh>,
    start: usize,
    size: usize,
    facet: usize,
) -> [Bound<'_, numpy::PyArray1<Float>>; 3] {
    let mesh = slf.borrow();
    let mesh = mesh.inner.borrow();
    let indices = mesh.get_facet_indices(facet).map(|i| i as usize);
    let arr = vertex_matrix(&mesh);
    let arr = arr.slice(ndarray::s![.., start..start + size]);

    let arr0 = arr.slice(ndarray::s![indices[0], ..]);
    let arr1 = arr.slice(ndarray::s![indices[1], ..]);
    let arr2 = arr.slice(ndarray::s![indices[2], ..]);

    unsafe {
        [
            numpy::PyArray1::borrow_from_array(&arr0, slf.clone().into_any()),
            numpy::PyArray1::borrow_from_array(&arr1, slf.clone().into_any()),
            numpy::PyArray1::borrow_from_array(&arr2, slf.into_any()),
        ]
    }
}

#[pyfunction]
pub fn load_image(path: &str) -> ((u32, u32), Vec<u8>) {
    let img = crate::mesh::load_image(path);
    (img.dimensions(), img.into_bytes())
}

#[pyfunction]
pub fn normal_facet<'py>(
    py: Python<'py>,
    ab: [Float; 3],
    ac: [Float; 3],
) -> Bound<'py, numpy::PyArray1<Float>> {
    crate::mesh::normal_facet(&Vec3::from_slice(&ab), &Vec3::from_slice(&ac))
        .to_array()
        .to_pyarray(py)
}

#[pyfunction]
pub fn area_facet(ab: [Float; 3], ac: [Float; 3]) -> PyResult<Float> {
    Ok(crate::mesh::area_facet(&Vec3::from_slice(&ab), &Vec3::from_slice(&ac)) as _)
}

#[pyfunction]
pub fn is_point_in_or_on(
    p1: [Float; 3],
    p2: [Float; 3],
    a: [Float; 3],
    b: [Float; 3],
) -> PyResult<bool> {
    Ok(crate::mesh::is_point_in_or_on(
        &p1.into(),
        &p2.into(),
        &a.into(),
        &b.into(),
    ))
}

#[pyfunction]
pub fn is_point_in_or_on_triangle(
    p: [Float; 3],
    a: [Float; 3],
    b: [Float; 3],
    c: [Float; 3],
) -> PyResult<bool> {
    Ok(crate::mesh::is_point_in_or_on_triangle(
        &p.into(),
        &a.into(),
        &b.into(),
        &c.into(),
    ))
}

#[pyfunction]
pub fn is_facing_plane<'py>(u: [Float; 3], n: [Float; 3]) -> PyResult<bool> {
    Ok(crate::mesh::is_facing_plane(&u.into(), &n.into()))
}

#[pyfunction]
pub fn is_not_parallel_to_plane<'py>(u: [Float; 3], n: [Float; 3]) -> PyResult<bool> {
    Ok(crate::mesh::is_not_parallel_to_plane(&u.into(), &n.into()))
}

#[pyfunction]
#[pyo3(signature = (p, u, a, n))]
pub fn intersect_plane<'py>(
    py: Python<'py>,
    p: [Float; 3],
    u: [Float; 3],
    a: [Float; 3],
    n: [Float; 3],
) -> Option<Bound<'py, numpy::PyArray1<Float>>> {
    crate::mesh::intersect_plane(&p.into(), &u.into(), &a.into(), &n.into())
        .map(|v| v.to_array().to_pyarray(py))
}

#[pyfunction]
#[pyo3(signature = (p, u, a, b, c, n))]
pub fn intersect_triangle<'py>(
    py: Python<'py>,
    p: [Float; 3],
    u: [Float; 3],
    a: [Float; 3],
    b: [Float; 3],
    c: [Float; 3],
    n: [Float; 3],
) -> Option<Bound<'py, numpy::PyArray1<Float>>> {
    crate::mesh::intersect_triangle(
        &p.into(),
        &u.into(),
        &a.into(),
        &b.into(),
        &c.into(),
        &n.into(),
    )
    .map(|v| v.to_array().to_pyarray(py))
}

#[pyfunction]
#[pyo3(signature = (p, u, a, b, c))]
pub fn intersect_triangle_moller_trumblore<'py>(
    py: Python<'py>,
    p: [Float; 3],
    u: [Float; 3],
    a: [Float; 3],
    b: [Float; 3],
    c: [Float; 3],
) -> Option<Bound<'py, numpy::PyArray1<Float>>> {
    crate::mesh::intersect_triangle_moller_trumbore(
        &p.into(),
        &u.into(),
        &a.into(),
        &b.into(),
        &c.into(),
    )
    .map(|v| v.to_array().to_pyarray(py))
}

#[pyfunction]
#[pyo3(signature = (mesh, p, u, exit_first=false))]
pub fn intersect_mesh<'py>(
    mesh: Bound<'py, Mesh>,
    p: [Float; 3],
    u: [Float; 3],
    exit_first: bool,
) -> Option<(usize, Bound<'py, numpy::PyArray1<Float>>)> {
    crate::mesh::intersect_mesh(
        &mesh.borrow().inner.borrow(),
        &p.into(),
        &u.into(),
        exit_first,
    )
    .map(|(ii, v)| (ii, v.to_array().to_pyarray(mesh.py())))
}

#[pyfunction]
pub fn view_factor_facets(
    face_a: Bound<'_, Facet>,
    face_b: Bound<'_, Facet>,
    trans_b2a: numpy::PyReadonlyArray2<'_, Float>,
) -> Float {
    crate::mesh::view_factor_facets(
        &face_a.borrow().inner.borrow(),
        &face_b.borrow().inner.borrow(),
        &crate::Mat4::from_cols_slice(trans_b2a.as_slice().unwrap()).transpose(),
    )
}

#[pyfunction]
pub fn rms_slope_terrain(
    theta: numpy::PyReadonlyArray1<'_, Float>,
    a: numpy::PyReadonlyArray1<'_, Float>,
) -> PyResult<Float> {
    Ok(crate::mesh::rms_slope_terrain(
        theta.as_array(),
        a.as_array(),
    ))
}
