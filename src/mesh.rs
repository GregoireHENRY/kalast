use glam::Vec4Swizzles;
use pyo3::prelude::*;

use crate::{Float, Mat4, Vec2, Vec3};

// std::mem::size_of::<Vertex>() / 8 = 18
#[cfg(feature = "use_f64")]
pub const VERTEX_STRIDE: usize = 18;

// std::mem::size_of::<Vertex>() / 4 = 19
#[cfg(not(feature = "use_f64"))]
pub const VERTEX_STRIDE: usize = 19;

pub const POS_OFFSET: usize = 0;
pub const TEX_OFFSET: usize = 3;
pub const NORMAL_OFFSET: usize = 5;
pub const TANGENT_OFFSET: usize = 8;
pub const BITANGENT_OFFSET: usize = 11;
pub const COLOR_OFFSET: usize = 14;
pub const COLOR_MODE_OFFSET: usize = 17;
// pub const EXTRA_OFFSET: usize = 17.5;

pub const MESH_CUBE: &'static str = include_str!("../res/cube.obj");

// pub const EPSILON_INTERSECT_TRIANGLE: Float = 1e-3;

// getter glam::Vec3 to numpy
// #[getter]
// pub fn a<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<f32>> {
//     let mut v = Array1::zeros(3);
//     for (i, v_) in v.iter_mut().enumerate() {
//         *v_ = self.a[i];
//     }
//     v.to_pyarray(py)
// }

// getter and setter from glam::Vec3 to [f32; 3]
// #[getter]
// pub fn get_camera_pos(&self) -> PyResult<[f32; 3]> {
//     Ok(self.camera_pos.into())
// }
//
// #[setter]
// pub fn set_camera_pos(&mut self, pos: [f32; 3]) -> PyResult<()> {
//     self.camera_pos.x = pos[0];
//     self.camera_pos.y = pos[1];
//     self.camera_pos.z = pos[2];
//     Ok(())
// }

// Convert `glam::Vec3` into Python
// fn into_py_glam_vec3<'py>(v: glam::Vec3, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
//     v.to_array().into_bound_py_any(py)
// }

// Do not reorder Vertex fields without updating offsets in GPU code and Pyo3 POD bindings.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: Vec3,
    pub tex: Vec2,
    pub normal: Vec3,
    pub tangent: Vec3,
    pub bitangent: Vec3,
    pub color: Vec3,
    pub color_mode: u32,
    pub extra: u32,
}

impl Vertex {
    // Need const default for GPU code so we don't use derive Default which is not const.
    pub const fn default() -> Self {
        Self {
            pos: Vec3::new(0.0, 0.0, 0.0),
            tex: Vec2::new(0.0, 0.0),
            normal: Vec3::new(0.0, 0.0, 0.0),
            tangent: Vec3::new(0.0, 0.0, 0.0),
            bitangent: Vec3::new(0.0, 0.0, 0.0),
            color: Vec3::new(1.0, 1.0, 1.0),
            color_mode: 0,
            extra: 0,
        }
    }
}

impl std::fmt::Debug for Vertex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Vertex(pos={}, tex={}, normal={}, tangent={}, bitangent={}, color={}, color_mode={})",
            self.pos,
            self.tex,
            self.normal,
            self.tangent,
            self.bitangent,
            self.color,
            self.color_mode,
        )
    }
}

#[repr(C)]
#[derive(Copy, Clone, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Facet {
    pub pos: Vec3,
    pub normal: Vec3,
    pub area: Float,
}

impl Facet {
    // Need const default for GPU code so we don't use derive Default which is not const.
    pub const fn default() -> Self {
        Self {
            pos: Vec3::new(0.0, 0.0, 0.0),
            normal: Vec3::new(0.0, 0.0, 0.0),
            area: 0.0,
        }
    }
}

impl std::fmt::Debug for Facet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Facet(pos={}, normal={}, area={})",
            self.pos, self.normal, self.area,
        )
    }
}

pub fn load_image<P>(path: P) -> image::DynamicImage
where
    P: AsRef<std::path::Path>,
{
    let bytes = std::fs::read(path).unwrap();
    image::load_from_memory(&bytes).unwrap()
}

pub fn load_image_from_obj<P>(path: P, texture: String) -> image::DynamicImage
where
    P: AsRef<std::path::Path>,
{
    let path = path.as_ref().parent().unwrap().join(texture);
    load_image(path)
}

#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct Material {
    pub diffuse: image::DynamicImage,
    pub normal: image::DynamicImage,
    // pub specular: image::DynamicImage,
}

impl Material {
    pub fn load<P>(path_obj: P, mat: &tobj::Material) -> Self
    where
        P: AsRef<std::path::Path>,
    {
        let path = path_obj.as_ref();
        let parent = path.parent().unwrap();

        let diffuse = load_image(parent.join(mat.diffuse_texture.as_ref().unwrap()));
        let normal = load_image(parent.join(mat.normal_texture.as_ref().unwrap()));

        Self {
            diffuse: diffuse,
            normal: normal,
        }
    }
}

#[repr(C)]
#[derive(Clone, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub facets: Vec<Facet>,
    pub material_id: Option<usize>,

    // temporary until better solution is found
    pub(crate) _vertices_before_flatten: Vec<Vertex>,
}

impl Mesh {
    pub const fn new() -> Self {
        Self {
            vertices: vec![],
            indices: vec![],
            facets: vec![],
            material_id: None,
            _vertices_before_flatten: vec![],
        }
    }

    pub fn load<P, F>(path: P, update_pos: F) -> Self
    where
        P: AsRef<std::path::Path>,
        F: Fn(Vec3) -> Vec3,
    {
        let Model { mut meshes, .. } = Model::load(path, update_pos);
        meshes.drain(0..1).next().unwrap()
    }

    fn __load_with_data<F>(
        _positions: Vec<f32>,
        _indices: Vec<u32>,
        _texcoords: Vec<f32>,
        _normals: Vec<f32>,
        _update_pos: F,
    ) -> Self
    where
        F: Fn(glam::Vec3) -> glam::Vec3,
    {
        unimplemented!();

        /*

        let mut vertices = (0..positions.len() / 3)
            .map(|i| {
                let pos = update_pos(
                    [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]].into(),
                );

                let mut v = Vertex {
                    pos,
                    ..Vertex::default()
                };
                if !texcoords.is_empty() {
                    v.tex = [texcoords[i * 2], 1.0 - texcoords[i * 2 + 1]].into();
                }
                if !normals.is_empty() {
                    v.normal = [normals[i * 3], normals[i * 3 + 1], normals[i * 3 + 2]].into();
                }
                v
            })
            .collect::<Vec<_>>();

        // Calculate normals per facet if normals per vertex not computed and texcoords are not provided.
        // When texcoords are provided, we use tangent and bitangent as calculated just above.
        let mut facets: Vec<Facet> = vec![];
        if texcoords.is_empty() {
            if normals.is_empty() {
                for (f, c) in indices.chunks(3).enumerate() {
                    let a = vertices[c[0] as usize].pos;
                    let b = vertices[c[1] as usize].pos;
                    let c = vertices[c[2] as usize].pos;

                    let p = (a + b + c) / 3.0;

                    let ab = b - a;
                    let ac = c - a;
                    let n = normal_facet(&ab, &ac);

                    let area = area_facet(&ab, &ac);

                    facets.push(Facet { p, n, area })
                }
            }
        }
        // Calculate tangents and bitangets for texture normal mapping.
        // We're going to use the triangles, so we need to loop through the indices in chunks of 3.
        else {
            let mut triangles_included = vec![0; vertices.len()];

            for c in indices.chunks(3) {
                let v0 = vertices[c[0] as usize];
                let v1 = vertices[c[1] as usize];
                let v2 = vertices[c[2] as usize];

                let pos0 = v0.pos;
                let pos1 = v1.pos;
                let pos2 = v2.pos;

                let uv0 = v0.tex;
                let uv1 = v1.tex;
                let uv2 = v2.tex;

                // Calculate the edges of the triangle
                let delta_pos1 = pos1 - pos0;
                let delta_pos2 = pos2 - pos0;

                // This will give us a direction to calculate the
                // tangent and bitangent
                let delta_uv1 = uv1 - uv0;
                let delta_uv2 = uv2 - uv0;

                // Solving the following system of equations will
                // give us the tangent and bitangent.
                //     delta_pos1 = delta_uv1.x * T + delta_u.y * B
                //     delta_pos2 = delta_uv2.x * T + delta_uv2.y * B
                // Luckily, the place I found this equation provided
                // the solution!
                let r = 1.0 / (delta_uv1.x * delta_uv2.y - delta_uv1.y * delta_uv2.x);
                let tangent = (delta_pos1 * delta_uv2.y - delta_pos2 * delta_uv1.y) * r;
                // We flip the bitangent to enable right-handed normal
                // maps with wgpu texture coordinate system
                let bitangent = (delta_pos2 * delta_uv1.x - delta_pos1 * delta_uv2.x) * -r;

                // We'll use the same tangent/bitangent for each vertex in the triangle
                vertices[c[0] as usize].tangent =
                    (tangent + glam::Vec3::from(vertices[c[0] as usize].tangent)).into();
                vertices[c[1] as usize].tangent =
                    (tangent + glam::Vec3::from(vertices[c[1] as usize].tangent)).into();
                vertices[c[2] as usize].tangent =
                    (tangent + glam::Vec3::from(vertices[c[2] as usize].tangent)).into();
                vertices[c[0] as usize].bitangent =
                    (bitangent + glam::Vec3::from(vertices[c[0] as usize].bitangent)).into();
                vertices[c[1] as usize].bitangent =
                    (bitangent + glam::Vec3::from(vertices[c[1] as usize].bitangent)).into();
                vertices[c[2] as usize].bitangent =
                    (bitangent + glam::Vec3::from(vertices[c[2] as usize].bitangent)).into();

                // Used to average the tangents/bitangents
                triangles_included[c[0] as usize] += 1;
                triangles_included[c[1] as usize] += 1;
                triangles_included[c[2] as usize] += 1;
            }

            // Average the tangents/bitangents
            for (i, n) in triangles_included.into_iter().enumerate() {
                let denom = 1.0 / n as f32;
                let v = &mut vertices[i];
                v.tangent = (glam::Vec3::from(v.tangent) * denom).into();
                v.bitangent = (glam::Vec3::from(v.bitangent) * denom).into();
            }
        }

        let mut mesh = Self {
            vertices,
            indices,
            facets,
            material_id: None,

            // temporary until better solution is found
            _vertices_before_flatten: vec![],
        };

        // Can now use normals per facet (if computed) to compute normals per vertex.
        if !mesh.facets.is_empty() {
            mesh.smoothen();
        }

        mesh.flatten();

        mesh
        */
    }

    // Take normals per facet and straight apply them to vertices per facet.
    // Also duplicate vertex to follow indices.
    pub fn flatten(&mut self) {
        // temporary until better solution is found
        self._vertices_before_flatten = self.vertices.clone();

        let mut new = vec![];

        for (fi, fv) in self.indices.chunks(3).enumerate() {
            self.vertices[fv[0] as usize].normal = self.facets[fi].normal;
            self.vertices[fv[1] as usize].normal = self.facets[fi].normal;
            self.vertices[fv[2] as usize].normal = self.facets[fi].normal;

            new.push(self.vertices[fv[0] as usize]);
            new.push(self.vertices[fv[1] as usize]);
            new.push(self.vertices[fv[2] as usize]);
        }

        self.vertices = new;
    }

    // Re-create vertices by removing duplicates (if it had been flatten before).
    // Compute normals per vertex using normals per facet averaged.
    pub fn smoothen(&mut self) {
        // re-create vertices if flatten (detected if as many vertices as indices)

        // temporary until better solution is found
        if !self._vertices_before_flatten.is_empty() {
            self.vertices = self._vertices_before_flatten.drain(..).collect();
        }

        // this could be the better solution is removing dups works, but im not sure, need tests
        /*
        if self.vertices.len() == self.indices.len() {
            let mut dups = vec![];
            for v in self.vertices.drain(..) {
                if !dups.contains(&v) {
                    dups.push(v);
                }
            }
            self.vertices = dups;
        }
        */

        // reset normals per vertex
        for ii in 0..self.vertices.len() {
            self.vertices[ii].normal = Vec3::ZERO;
        }

        // add surrounding normals per facet
        for (fi, fv) in self.indices.chunks(3).enumerate() {
            self.vertices[fv[0] as usize].normal += self.facets[fi].normal;
            self.vertices[fv[1] as usize].normal += self.facets[fi].normal;
            self.vertices[fv[2] as usize].normal += self.facets[fi].normal;
        }

        // normalize to get average
        for ii in 0..self.vertices.len() {
            self.vertices[ii].normal = self.vertices[ii].normal.normalize();
        }
    }

    pub fn is_flat(&self) -> bool {
        // temporary until better solution is found
        !self._vertices_before_flatten.is_empty()
    }

    pub fn get_facet_vertices(&self, facet: usize) -> [&Vertex; 3] {
        if self.is_flat() {
            self.vertices
                .chunks(3)
                .map(|c| [&c[0], &c[1], &c[2]])
                .skip(facet)
                .next()
                .unwrap()
        } else {
            self.get_facet_indices(facet)
                .map(|ii| &self.vertices[ii as usize])
        }
    }

    pub fn get_facet_indices(&self, facet: usize) -> [u32; 3] {
        self.indices
            .chunks(3)
            .map(|c| [c[0], c[1], c[2]])
            .skip(facet)
            .next()
            .unwrap()
    }

    pub fn get_facet_positions(&self, facet: usize) -> [&Vec3; 3] {
        self.get_facet_vertices(facet).map(|v| &v.pos)
    }

    pub fn get_facet_normals(&self, facet: usize) -> [&Vec3; 3] {
        self.get_facet_vertices(facet).map(|v| &v.normal)
    }

    pub fn update_all_vertices_colors(&mut self, mode: u32, color: Vec3) {
        for v in &mut self.vertices {
            v.color_mode = mode;
            v.color = color;
        }
    }

    pub fn intersect(&self, p: &Vec3, u: &Vec3, exit_first: bool) -> Option<(usize, Vec3)> {
        intersect_mesh(self, p, u, exit_first)
    }
}

impl std::fmt::Debug for Mesh {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Mesh(vertices={:?}, indices={:?}, facets={:?}, material_id={}",
            self.vertices,
            self.indices,
            self.facets,
            self.material_id
                .map_or("None".to_string(), |id: usize| id.to_string()),
        )
    }
}

#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct Model {
    pub meshes: Vec<Mesh>,
    pub materials: Vec<Material>,
}

impl Model {
    pub fn load<P, F>(path: P, update_pos: F) -> Self
    where
        P: AsRef<std::path::Path>,
        F: Fn(Vec3) -> Vec3,
    {
        let path = path.as_ref();
        println!("loading model: {:?}", path);

        let obj_text = std::fs::read_to_string(path).unwrap();
        let obj_cursor = std::io::Cursor::new(obj_text);
        let mut obj_reader = std::io::BufReader::new(obj_cursor);

        let (models, obj_materials) = tobj::load_obj_buf(
            &mut obj_reader,
            &tobj::LoadOptions {
                triangulate: true,
                single_index: true,
                ..Default::default()
            },
            |p| {
                let p = path.parent().unwrap().join(p);
                let mat_text = std::fs::read_to_string(p).unwrap();
                tobj::load_mtl_buf(&mut std::io::BufReader::new(std::io::Cursor::new(mat_text)))
            },
        )
        .unwrap();

        let materials = obj_materials
            .unwrap()
            .iter()
            .map(|mat| Material::load(path, mat))
            .collect();

        let meshes = models
            .into_iter()
            .map(|m| {
                let tobj::Model { mesh, .. } = m;
                let tobj::Mesh {
                    positions,
                    texcoords,
                    normals,
                    indices,
                    material_id,
                    ..
                } = mesh;

                let mut vertices = (0..positions.len() / 3)
                    .map(|i| {
                        let pos = update_pos(
                            [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]].into(),
                        );

                        let mut v = Vertex {
                            pos,
                            ..Vertex::default()
                        };
                        if !texcoords.is_empty() {
                            v.tex = [texcoords[i * 2], 1.0 - texcoords[i * 2 + 1]].into();
                        }
                        if !normals.is_empty() {
                            println!("why did it reach here?");
                            v.normal =
                                [normals[i * 3], normals[i * 3 + 1], normals[i * 3 + 2]].into();
                        }
                        v
                    })
                    .collect::<Vec<_>>();

                // Calculate normals per facet if normals per vertex not computed and texcoords are not provided.
                // When texcoords are provided, we use tangent and bitangent as calculated just above.

                let mut facets: Vec<Facet> = vec![];
                if texcoords.is_empty() {
                    if normals.is_empty() {
                        for fv in indices.chunks(3) {
                            let a = vertices[fv[0] as usize].pos;
                            let b = vertices[fv[1] as usize].pos;
                            let c = vertices[fv[2] as usize].pos;

                            let pos = (a + b + c) / 3.0;

                            let ab = b - a;
                            let ac = c - a;
                            let normal = normal_facet(&ab, &ac);
                            let area = area_facet(&ab, &ac);

                            // println!(
                            //     "calc facet {} ({}, {}, {}): normal={}",
                            //     fi, fv[0], fv[1], fv[2], n
                            // );

                            facets.push(Facet { pos, normal, area })
                        }
                    }
                }
                // Calculate tangents and bitangets for texture normal mapping.
                // We're going to use the triangles, so we need to loop through the indices in chunks of 3.
                else {
                    let mut triangles_included = vec![0; vertices.len()];

                    for c in indices.chunks(3) {
                        let v0 = vertices[c[0] as usize];
                        let v1 = vertices[c[1] as usize];
                        let v2 = vertices[c[2] as usize];

                        let pos0 = v0.pos;
                        let pos1 = v1.pos;
                        let pos2 = v2.pos;

                        let uv0 = v0.tex;
                        let uv1 = v1.tex;
                        let uv2 = v2.tex;

                        // Calculate the edges of the triangle
                        let delta_pos1 = pos1 - pos0;
                        let delta_pos2 = pos2 - pos0;

                        // This will give us a direction to calculate the
                        // tangent and bitangent
                        let delta_uv1 = uv1 - uv0;
                        let delta_uv2 = uv2 - uv0;

                        // Solving the following system of equations will
                        // give us the tangent and bitangent.
                        //     delta_pos1 = delta_uv1.x * T + delta_u.y * B
                        //     delta_pos2 = delta_uv2.x * T + delta_uv2.y * B
                        // Luckily, the place I found this equation provided
                        // the solution!
                        let r = 1.0 / (delta_uv1.x * delta_uv2.y - delta_uv1.y * delta_uv2.x);
                        let tangent = (delta_pos1 * delta_uv2.y - delta_pos2 * delta_uv1.y) * r;
                        // We flip the bitangent to enable right-handed normal
                        // maps with wgpu texture coordinate system
                        let bitangent = (delta_pos2 * delta_uv1.x - delta_pos1 * delta_uv2.x) * -r;

                        // We'll use the same tangent/bitangent for each vertex in the triangle
                        vertices[c[0] as usize].tangent =
                            (tangent + Vec3::from(vertices[c[0] as usize].tangent)).into();
                        vertices[c[1] as usize].tangent =
                            (tangent + Vec3::from(vertices[c[1] as usize].tangent)).into();
                        vertices[c[2] as usize].tangent =
                            (tangent + Vec3::from(vertices[c[2] as usize].tangent)).into();
                        vertices[c[0] as usize].bitangent =
                            (bitangent + Vec3::from(vertices[c[0] as usize].bitangent)).into();
                        vertices[c[1] as usize].bitangent =
                            (bitangent + Vec3::from(vertices[c[1] as usize].bitangent)).into();
                        vertices[c[2] as usize].bitangent =
                            (bitangent + Vec3::from(vertices[c[2] as usize].bitangent)).into();

                        // Used to average the tangents/bitangents
                        triangles_included[c[0] as usize] += 1;
                        triangles_included[c[1] as usize] += 1;
                        triangles_included[c[2] as usize] += 1;
                    }

                    // Average the tangents/bitangents
                    for (i, n) in triangles_included.into_iter().enumerate() {
                        let denom = 1.0 / n as Float;
                        let v = &mut vertices[i];
                        v.tangent = v.tangent * denom;
                        v.bitangent = v.bitangent * denom;
                    }
                }

                let mut mesh = Mesh {
                    vertices,
                    indices,
                    facets,
                    material_id,

                    // temporary until better solution is found
                    _vertices_before_flatten: vec![],
                };

                // Can now use normals per facet (if computed) to compute normals per vertex.
                if !mesh.facets.is_empty() {
                    mesh.smoothen();
                }

                mesh
            })
            .collect();

        Self { meshes, materials }
    }
}

pub fn normal_facet(ab: &Vec3, ac: &Vec3) -> Vec3 {
    // ab: b - a
    // ac: c - a
    ab.cross(*ac).normalize()
}

pub fn area_facet(ab: &Vec3, ac: &Vec3) -> Float {
    // ab: b - a
    // ac: c - a
    0.5 * ab.angle_between(*ac).sin() * ab.length() * ac.length()
}

pub fn is_point_in_or_on(p1: &Vec3, p2: &Vec3, a: &Vec3, b: &Vec3) -> bool {
    let cp1 = (b - a).cross(p1 - a);
    let cp2 = (b - a).cross(p2 - a);
    cp1.dot(cp2) >= 0.0
}

pub fn is_point_in_or_on_triangle(p: &Vec3, a: &Vec3, b: &Vec3, c: &Vec3) -> bool {
    // In triangle means if it resides within the boundaries of the triangle, not the plane, the 3d space "above" and "below".
    // a, b, c: vertices triangle
    is_point_in_or_on(p, a, b, c) && is_point_in_or_on(p, b, c, a) && is_point_in_or_on(p, c, a, b)
}

pub fn is_facing_plane(u: &Vec3, n: &Vec3) -> bool {
    // u: raydir
    // n: normal plane
    u.dot(*n) <= 0.0
}

pub fn is_not_parallel_to_plane(u: &Vec3, n: &Vec3) -> bool {
    // u: raydir
    // n: normal plane
    let det = u.dot(*n);
    det < -1e-7 || det > 1e-7
}

pub fn intersect_plane(p: &Vec3, u: &Vec3, a: &Vec3, n: &Vec3) -> Option<Vec3> {
    // p: raystart
    // u: raydir
    // a, b, c: vertices triangle
    // n: normal plane
    // is_facing_plane(u, n)
    is_not_parallel_to_plane(u, n).then_some(p + u * (a - p).dot(*n) / u.dot(*n))
}

pub fn intersect_triangle(
    p: &Vec3,
    u: &Vec3,
    a: &Vec3,
    b: &Vec3,
    c: &Vec3,
    n: &Vec3,
) -> Option<Vec3> {
    // p: raystart
    // u: raydir
    // a, b, c: edges triangle
    // n: normal plane
    intersect_plane(p, u, a, n).and_then(|x| is_point_in_or_on_triangle(&x, a, b, c).then_some(x))
}

// Möller–Trumbore intersection algorithm
pub fn intersect_triangle_moller_trumbore(
    p: &Vec3,
    u: &Vec3,
    a: &Vec3,
    b: &Vec3,
    c: &Vec3,
) -> Option<Vec3> {
    let e1 = b - a;
    let e2 = c - a;

    let ray_cross_e2 = u.cross(e2);
    let det = e1.dot(ray_cross_e2);

    // println!("det={} p={} u={} n={}", det, p, u, n);
    // println!("p={} u={} n={}", p, u, n);

    // test ray parallel to triangle
    if det > -crate::util::EPSILON && det < crate::util::EPSILON {
        // println!("PARALLEL det={}", det);
        return None;
    }
    // println!("NOT PARALLEL det={}", det);

    // test ray not facing triangle
    // this is not part of the original Möller–Trumbore algo
    // what is the difference with the t < 0 test at the end of the function?
    // u is ray and n is normal
    // if !is_facing_plane(u, n) {
    //     println!("NOT FACING");
    //     return None;
    // }
    // println!("FACING");

    let inv_det = 1.0 / det;
    let s = p - a;
    let bu = inv_det * s.dot(ray_cross_e2);
    if bu < 0.0 || bu > 1.0 {
        return None;
    }

    let s_cross_e1 = s.cross(e1);
    let bv = inv_det * u.dot(s_cross_e1);
    if bv < 0.0 || bu + bv > 1.0 {
        return None;
    }

    // At this stage we can compute t to find out where the intersection point is on the line.
    let t = inv_det * e2.dot(s_cross_e1);

    // println!("t={}, u={}, n={}", t, u, n);

    if t > crate::util::EPSILON {
        // ray intersection
        // println!("lucky!!");
        let intersection_point = p + u * t;
        return Some(intersection_point);
    } else {
        // This means that there is a line intersection but not a ray intersection.
        // println!("unlucky..");
        return None;
    }
}

/* Put that in for loop after `intersect` calculation to debug
println!(
    "intersected #{} start: {:?}, intersected: {:?}, dist: {}, best dist found: {:?}",
    ii,
    p.as_slice(),
    intersect.as_slice(),
    (intersect - p).magnitude(),
    best_intersect
        .as_ref()
        .and_then(|i| Some((i.0 - p).magnitude()))
);
*/
pub fn intersect_mesh(mesh: &Mesh, p: &Vec3, u: &Vec3, exit_first: bool) -> Option<(usize, Vec3)> {
    let mut best_intersect: Option<(usize, Vec3)> = None;
    let mut best_dist: Option<Float> = None;

    let it: Vec<(&Vec3, &Vec3, &Vec3)> = {
        if mesh.is_flat() {
            mesh.vertices
                .chunks(3)
                .map(|c| (&c[0].pos, &c[1].pos, &c[2].pos))
                .collect()
        } else {
            mesh.indices
                .chunks(3)
                .map(|c| {
                    (
                        &mesh.vertices[c[0] as usize].pos,
                        &mesh.vertices[c[1] as usize].pos,
                        &mesh.vertices[c[2] as usize].pos,
                    )
                })
                .collect()
        }
    };

    // println!("{} {}", p, u);

    for (f, (a, b, c)) in it.iter().enumerate() {
        // println!("{}: {}, {}, {}", f, a, b, c);
        // &mesh.facets[f].n
        if let Some(intersect) = intersect_triangle_moller_trumbore(p, u, a, b, c) {
            let dist = (intersect - p).length();
            // println!("found! dist={}", dist);

            if let None = best_intersect {
                best_intersect = Some((f, intersect));
                best_dist = Some(dist);

                if exit_first {
                    return best_intersect;
                }
            } else if let Some(best_dist) = &mut best_dist {
                if dist < *best_dist {
                    best_intersect = Some((f, intersect));
                    *best_dist = dist;
                }
            }
        };
    }

    best_intersect
}

/// Compute the view factor between a facet A and B with area of facet B.
#[pyfunction]
pub fn view_factor_scalar_with_area(
    area_b: Float,
    angle_at_a: Float,
    angle_at_b: Float,
    distance_a2b: Float,
) -> Float {
    area_b * view_factor_scalar(angle_at_a, angle_at_b, distance_a2b)
}

/// View factor between facet A and B but without area of facet B.
/// You can actually multiply by the area of facet A instead of B if A is transmitting energy to B.
#[pyfunction]
pub fn view_factor_scalar(angle_at_a: Float, angle_at_b: Float, distance_a2b: Float) -> Float {
    angle_at_a.cos() * angle_at_b.cos() / (crate::util::PI * distance_a2b.powi(2))
}

/// Compute the view factor between facet A and B.
/// Both facets have coordinates in their fixed frames.
///
/// trans_b2a is the model matrix to transform from the fixed frame of the body of facet B,
/// to the fixed frame of the body of facet A.
/// it is used to express coordinates of facet B in fixed frame of body A (applied to facet B).
///
/// It is the view factor by unit of area, multiply either by the area of facet A or B when you know which one is
/// transmitting energy to the other one.
pub fn view_factor_facets(face_a: &Facet, face_b: &Facet, trans_b2a: &Mat4) -> Float {
    // Vector from center of facet **A** to facet **B**.
    let vector_a2b = (trans_b2a * face_b.pos.extend(1.0)).xyz() - face_a.pos;
    let distance_a2b = vector_a2b.length();
    let unit_a2b = vector_a2b.normalize();

    // This is a condition on the relation between distance of the two facets and their surface area to avoid too large
    // view factor in case of very close distance.
    // TODO: subdivide the facet and recompute.
    if distance_a2b < face_b.area.sqrt() {
        return 0.0;
    }

    // Angles from both normals and the unit vector to the other facet.
    // The normal of facet B needs to be transformed to fixed-frame A for correct calculation.
    let angle_at_a = face_a.normal.angle_between(unit_a2b);
    let angle_at_b = trans_b2a
        .transform_vector3(face_b.normal)
        .angle_between(-unit_a2b);

    // Another condition is one that was actually mentioned earlier: angles must be smaller than 90°.
    if angle_at_a >= crate::util::PI / 2.0 || angle_at_b >= crate::util::PI / 2.0 {
        return 0.0;
    }

    // Well, ready for calculation.
    view_factor_scalar(angle_at_a, angle_at_b, distance_a2b)
}

/// Largest slope angle of spherical segment, in radian.
///
/// S: curvature diameter
#[allow(non_snake_case)]
#[pyfunction]
pub fn largest_slope_angle_sphere(S: Float) -> Float {
    (1.0 - 2.0 * S).acos()
}

/// Curvature diameter of spherical segment, in radian.
///
/// g: largest slope angle
#[allow(non_snake_case)]
#[pyfunction]
pub fn curvature_diameter_sphere(S: Float) -> Float {
    (1.0 - S.cos()) / 2.0
}

/// Curvature radius in a concave segment, in radian.
///
/// r: radius crater
/// d: depth crater
#[pyfunction]
pub fn curvature_radius(r: Float, d: Float) -> Float {
    (r.powi(2) + d.powi(2)) / (2.0 * d)
}

/// Curvature diameter from radius, in a concave segment, in radian.
///
/// R: curvature radius
/// d: depth crater
#[allow(non_snake_case)]
#[pyfunction]
pub fn curvature_diameter_from_radius(d: Float, R: Float) -> Float {
    d / (2.0 * R)
}

/// Z position inside crater
///
/// x, y: position
/// r: radius crater
/// d: depth crater
#[allow(non_snake_case)]
#[pyfunction]
pub fn z_in_crater(x: Float, y: Float, r: Float, d: Float) -> Float {
    let R = curvature_radius(r, d);
    R - d - (R.powi(2) - x.powi(2) - y.powi(2)).sqrt()
}

/// RMS slope, in radian
///
/// f: coverage
/// g: largest slope angle
#[pyfunction]
pub fn rms_slope(f: Float, g: Float) -> Float {
    (f / 2.0 * (g.powi(2) - (g * g.cos() - g.sin()).powi(2) / g.sin().powi(2))).sqrt()
}

/// RMS slope in case of hemispherical crater, in radian
///
/// f: coverage
#[pyfunction]
pub fn rms_slope_hemisphere(f: Float) -> Float {
    49.0 * f.sqrt()
}

/// RMS slope for a terrain, in radian
///
/// theta: angle between facet normal and average normal of terrain
/// a: facet area
pub fn rms_slope_terrain(
    theta: numpy::ndarray::ArrayView1<Float>,
    a: numpy::ndarray::ArrayView1<Float>,
) -> Float {
    let mut s1 = 0.0;
    let mut s2 = 0.0;
    for ii in 0..theta.len() {
        let b = a[ii] * theta[ii].cos();
        s1 += theta[ii].powi(2) * b;
        s2 += b;
    }
    (s1 / s2).sqrt()
}

#[pyfunction]
pub fn distribution_slope_angles(theta: Float, a: Float, b: Float) -> Float {
    a * (-theta.tan().powi(2) / b).exp() * theta.sin() / theta.cos().powi(2)
}
