use glam::Vec4Swizzles;
#[cfg(feature = "python")]
use pyo3::prelude::*;

use crate::{Float, Mat4, Vec3};

// In `Float`s: pos 3, normal 3, colour 3, then the mode as one slot --
// `u32` under f32, `u32` plus four bytes of padding under f64, which is why
// the stride is the same 10 either way.
//
// std::mem::size_of::<Vertex>() / 8 = 10 (f64), / 4 = 10 (f32)
pub const VERTEX_STRIDE: usize = 10;

pub const POS_OFFSET: usize = 0;
pub const NORMAL_OFFSET: usize = 3;
pub const COLOR_OFFSET: usize = 6;
pub const COLOR_MODE_OFFSET: usize = 9;

pub const MESH_CUBE: &'static str = include_str!("../res/cube.obj");

// pub const EPSILON_INTERSECT_TRIANGLE: Float = 1e-3;

// getter glam::Vec3 to numpy
// #[cfg_attr(feature = "python", getter)]
// pub fn a<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<f32>> {
//     let mut v = Array1::zeros(3);
//     for (i, v_) in v.iter_mut().enumerate() {
//         *v_ = self.a[i];
//     }
//     v.to_pyarray(py)
// }

// getter and setter from glam::Vec3 to [f32; 3]
// #[cfg_attr(feature = "python", getter)]
// pub fn get_camera_pos(&self) -> PyResult<[f32; 3]> {
//     Ok(self.camera_pos.into())
// }
//
// #[cfg_attr(feature = "python", setter)]
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

/// Axis-aligned bounding box, used to fit camera and light frustums to the
/// scene instead of making the user supply near/far/side by hand.
///
/// An empty box is `min > max` on every axis, so `union` with anything gives
/// that thing back and `is_empty` stays cheap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn empty() -> Self {
        Self {
            min: Vec3::splat(Float::INFINITY),
            max: Vec3::splat(Float::NEG_INFINITY),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x || self.min.y > self.max.y || self.min.z > self.max.z
    }

    pub fn from_vertices(vertices: &[Vertex]) -> Self {
        let mut aabb = Self::empty();
        for v in vertices {
            aabb.min = aabb.min.min(v.pos);
            aabb.max = aabb.max.max(v.pos);
        }
        aabb
    }

    pub fn union(&self, other: &Self) -> Self {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Half the length of the box diagonal -- the radius of the bounding
    /// sphere. Rotation-invariant, which is what makes it usable for a light
    /// frustum that has to stay stable as the sun moves.
    pub fn radius(&self) -> Float {
        if self.is_empty() {
            return 0.0;
        }
        (self.max - self.min).length() * 0.5
    }

    pub fn corners(&self) -> [Vec3; 8] {
        let (a, b) = (self.min, self.max);
        [
            Vec3::new(a.x, a.y, a.z),
            Vec3::new(b.x, a.y, a.z),
            Vec3::new(a.x, b.y, a.z),
            Vec3::new(b.x, b.y, a.z),
            Vec3::new(a.x, a.y, b.z),
            Vec3::new(b.x, a.y, b.z),
            Vec3::new(a.x, b.y, b.z),
            Vec3::new(b.x, b.y, b.z),
        ]
    }

    /// Bounding box of this box's corners after `mat`. Re-fitting the corners
    /// (rather than transforming min/max) keeps the result correct under
    /// rotation, at the cost of being conservative.
    pub fn transform(&self, mat: &Mat4) -> Self {
        if self.is_empty() {
            return *self;
        }
        let mut out = Self::empty();
        for c in self.corners() {
            let p = mat.transform_point3(c);
            out.min = out.min.min(p);
            out.max = out.max.max(p);
        }
        out
    }
}

/// One vertex, as both the CPU and the GPU see it.
///
/// It used to carry a texture coordinate, a tangent and a bitangent -- for
/// normal mapping, which no live shader does -- and an `extra` word nothing
/// ever read: 36 of its 76 bytes, on every vertex of every mesh, 340 MB of a
/// 3M-facet model. They are gone; a textured OBJ still loads, its texture
/// coordinates simply are not kept. See
/// `notes/2026-09-18_memory_meshes_and_shadow_maps.md`.
///
/// Do not reorder these without updating the offsets above, the GPU layout in
/// `app::gpu`, and the strides the Python views use.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: Vec3,
    pub normal: Vec3,
    pub color: Vec3,
    pub color_mode: u32,
    /// Only under `use_f64`, where the three vectors are 8-aligned and the
    /// struct would otherwise carry four bytes of padding -- which `Pod`
    /// forbids. Named rather than implied, so `bytemuck` can see it.
    #[cfg(feature = "use_f64")]
    pub _pad: u32,
}

impl Vertex {
    // Need const default for GPU code so we don't use derive Default which is not const.
    pub const fn default() -> Self {
        Self {
            pos: Vec3::new(0.0, 0.0, 0.0),
            normal: Vec3::new(0.0, 0.0, 0.0),
            color: Vec3::new(1.0, 1.0, 1.0),
            color_mode: 0,
            #[cfg(feature = "use_f64")]
            _pad: 0,
        }
    }
}

impl std::fmt::Debug for Vertex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Vertex(pos={}, normal={}, color={}, color_mode={})",
            self.pos, self.normal, self.color, self.color_mode,
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

    /// The shared topology `flatten` replaced, so `smoothen` can put it back.
    ///
    /// A flat mesh's `indices` are the identity `0..3f`, because its vertices
    /// are already triangle-major. Keeping that true rather than leaving the
    /// pre-flatten values in place is what lets every consumer read `indices`
    /// without first asking `is_flat()` -- and forgetting to ask is what made
    /// `recompute_facets` return NaN normals on a flattened mesh.
    pub(crate) _indices_before_flatten: Vec<u32>,

    // Set by mutating vertex color/color_mode/extra in place (e.g.
    // per-facet colormaps) to request a GPU re-upload on the next frame.
    // Starts false: the initial upload happens unconditionally when the
    // mesh is first loaded into a MeshBuffer, this flag only matters for
    // updates after that.
    pub colors_dirty: bool,

    // Model-space bounds, used to fit camera/light frustums. Computed once
    // per load rather than per frame -- a full pass over 9.4M vertices is
    // milliseconds at load time but would be nonsense every frame.
    // Call `recompute_bounds` after mutating vertex positions in place.
    /// Whether every facet owns its three vertices. Explicit, because it
    /// used to be inferred from the shared copy `flatten` keeps for
    /// `smoothen` -- and a mesh loaded flat keeps none.
    pub flat: bool,
    pub bounds: Aabb,

    /// The file this was loaded from, or `None` for a mesh built in memory.
    ///
    /// Kept so the editor can say *which* shape model a body is, which is
    /// the first thing anyone wants to know about a scene they did not
    /// build a minute ago. Every mesh in one `.obj` carries the same path.
    pub path: Option<std::path::PathBuf>,

    // Per-facet scalar to colour by -- a temperature, an insolation, a
    // shadowed fraction. Empty when the mesh is coloured by vertex colour
    // instead, which is the default.
    //
    // Per facet rather than per vertex because that is the shape the data
    // arrives in: a TPM column gives one surface temperature per facet, and
    // these meshes are flattened, so there is nothing to interpolate across
    // anyway -- the three corners of a facet are its own vertices.
    pub values: Vec<Float>,
}

impl Mesh {
    pub const fn new() -> Self {
        Self {
            vertices: vec![],
            indices: vec![],
            facets: vec![],
            material_id: None,
            _vertices_before_flatten: vec![],
            _indices_before_flatten: vec![],
            colors_dirty: false,
            flat: false,
            bounds: Aabb {
                min: Vec3::ZERO,
                max: Vec3::ZERO,
            },
            path: None,
            values: vec![],
        }
    }

    /// Recomputes model-space bounds from current vertex positions. Call
    /// after mutating positions in place, the same way `recompute_facets` is
    /// needed for facet data.
    pub fn recompute_bounds(&mut self) {
        self.bounds = Aabb::from_vertices(&self.vertices);
    }

    /// The default way in: a flat mesh built straight from the file's
    /// positions and triangles, with no shared mesh in between and nothing
    /// kept to go back to.
    ///
    /// Two costs went with the old route -- parse to a shared mesh, then
    /// `flatten` it -- that a flat mesh never needed: the shared vertices
    /// built, copied and kept (150 MB for a 3M-facet model), and a parse
    /// that ran on one core for 0.85 s of the 1.0 s a load took. Plain
    /// `v`/`f` triangle files, which is what shape models are, are parsed in
    /// parallel over the bytes and the flat vertices and facets are built in
    /// parallel from the result. Anything else the format allows -- texture
    /// coordinates, normals, materials, several objects, indices with
    /// slashes or negative -- goes through `load` and `flatten` exactly as
    /// before, minus the copy kept for `smoothen`. See
    /// `notes/2026-09-18_memory_meshes_and_shadow_maps.md`.
    pub fn load_flat<P, F>(path: P, update_pos: F) -> Self
    where
        P: AsRef<std::path::Path>,
        F: Fn(Vec3) -> Vec3,
    {
        let path = path.as_ref();
        let mut timer = LoadTimer::start();
        let Some((mut positions, tris)) = plain_mesh(path, &mut timer) else {
            let mut mesh = Self::load_via_tobj(path, update_pos);
            mesh.flatten();
            mesh._vertices_before_flatten = Vec::new();
            mesh._indices_before_flatten = Vec::new();
            return mesh;
        };
        println!("loading model: {:?}", path);
        for p in positions.iter_mut() {
            *p = update_pos(*p);
        }
        let (min, max) = positions.iter().fold(
            (Vec3::splat(Float::INFINITY), Vec3::splat(Float::NEG_INFINITY)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        );
        let (vertices, facets) = build_flat(&positions, &tris);
        drop(positions);
        timer.phase("build");
        let indices = (0..vertices.len() as u32).collect();
        timer.phase("indices");
        Mesh {
            indices,
            vertices,
            facets,
            material_id: None,
            _vertices_before_flatten: vec![],
            _indices_before_flatten: vec![],
            colors_dirty: false,
            flat: true,
            bounds: Aabb { min, max },
            path: Some(path.to_path_buf()),
            values: vec![],
        }
    }

    /// The shared mesh the file describes: its vertices, the triangles over
    /// them, and a normal per vertex averaged from the facets around it.
    ///
    /// Built directly for the plain `v`/`f` files shape models are -- parsed
    /// in parallel, vertices numbered by first appearance in the faces and
    /// unreferenced ones dropped, which is tobj's own order, so the result is
    /// bit-for-bit what tobj gave and `load_via_tobj` still gives for
    /// everything else. 0.94 s to 0.2 s on a 3M-facet model.
    pub fn load<P, F>(path: P, update_pos: F) -> Self
    where
        P: AsRef<std::path::Path>,
        F: Fn(Vec3) -> Vec3,
    {
        let path = path.as_ref();
        let mut timer = LoadTimer::start();
        let Some((mut positions, tris)) = plain_mesh(path, &mut timer) else {
            return Self::load_via_tobj(path, update_pos);
        };
        println!("loading model: {:?}", path);
        for p in positions.iter_mut() {
            *p = update_pos(*p);
        }
        let (vertices, indices, facets) = build_smooth(&positions, &tris);
        drop(positions);
        timer.phase("build");
        let bounds = Aabb::from_vertices(&vertices);
        timer.phase("bounds");
        Mesh {
            vertices,
            indices,
            facets,
            material_id: None,
            _vertices_before_flatten: vec![],
            _indices_before_flatten: vec![],
            colors_dirty: false,
            flat: false,
            bounds,
            path: Some(path.to_path_buf()),
            values: vec![],
        }
    }

    /// The same through tobj: the general OBJ reader, with texture
    /// coordinates, normals, materials and several objects. What `load`
    /// falls back to, and what its output is tested against.
    pub fn load_via_tobj<P, F>(path: P, update_pos: F) -> Self
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
            _indices_before_flatten: vec![],
            colors_dirty: false,
            flat: false,
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

        // The rebuilt vertices are triangle-major, so the shared indices no
        // longer address them: facet 1 kept pointing at rows 1, 3 and 4 where
        // its corners had moved to 3, 4 and 5. The renderer never noticed --
        // it draws a flat mesh sequentially and ignores the index buffer --
        // but `compute_facets` reads them, so `recompute_facets()` on a
        // flattened mesh produced NaN normals off a degenerate triangle.
        self._indices_before_flatten = std::mem::take(&mut self.indices);
        self.indices = (0..self.vertices.len() as u32).collect();
        self.flat = true;
    }

    // Re-create vertices by removing duplicates (if it had been flatten before).
    // Compute normals per vertex using normals per facet averaged.
    /// Back to shared corners with averaged normals. `false` when there is
    /// nothing to go back to: a mesh loaded flat keeps no shared topology
    /// (`load_flat`), and stays as it is -- load it with `smooth` instead.
    pub fn smoothen(&mut self) -> bool {
        if !self._vertices_before_flatten.is_empty() {
            self.vertices = self._vertices_before_flatten.drain(..).collect();
            // And the topology with them: the loop below walks `indices` to
            // average facet normals onto shared corners, which the identity
            // indices of a flat mesh cannot express.
            if !self._indices_before_flatten.is_empty() {
                self.indices = std::mem::take(&mut self._indices_before_flatten);
            }
            self.flat = false;
        } else if self.flat {
            return false;
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
        true
    }

    // Recompute facets (pos, normal, area) from current vertices positions and indices.
    // Call after mutating vertex positions in place, since facets are not kept in sync automatically.
    /// Facets whose normal points into the body rather than out of it.
    ///
    /// A decimated shape model can carry a handful of triangles with reversed
    /// winding, and they are quietly destructive. The thermophysical model
    /// clamps `cos(incidence)` at zero, so such a facet is permanently dark
    /// and sits at night temperature forever. A hemicube placed on one looks
    /// *into* the body and reports a self view factor near 1, which would
    /// pour a body's own thermal emission back into it.
    ///
    /// Detected by comparing each normal against the outward radial direction
    /// from the mesh centroid, which assumes a roughly star-shaped body.
    ///
    /// **This heuristic is unreliable, measured against the hemicube.** A
    /// facet that really is reversed sees its own body fill its hemisphere,
    /// so a self view factor near 1 is ground truth and needs no assumption
    /// about shape. Compared on the decimated Dimorphos models:
    ///
    /// | mesh | flagged here | self VF > 0.5 | agreeing |
    /// |---|---|---|---|
    /// | 10k | 22 | 1 | 1 |
    /// | 100k | 21 | 3 | 2 |
    ///
    /// So it over-reports by ~20x on these meshes, and on the 100k it also
    /// *misses* one (66473). Flipping what it reports makes things strictly
    /// worse: the 21 false positives are real concavities, and reversing them
    /// sends their self view factor from 0.24 to 1.0.
    ///
    /// Use it as a cheap pre-filter only. `flip_facets` is deliberately not
    /// wired to it.
    ///
    /// Worth knowing where these come from: the **full-resolution 3.1M
    /// Dimorphos and every Didymos model flag zero**. They are a decimation
    /// artefact, not a defect of the source shape models.
    pub fn inward_facing_facets(&self) -> Vec<u32> {
        if self.facets.is_empty() {
            return vec![];
        }
        let centre = self
            .facets
            .iter()
            .fold(Vec3::ZERO, |acc, f| acc + f.pos)
            / self.facets.len() as Float;

        self.facets
            .iter()
            .enumerate()
            .filter_map(|(i, f)| {
                let out = (f.pos - centre).normalize_or_zero();
                (out.length_squared() > 0.5 && f.normal.dot(out) < 0.0).then_some(i as u32)
            })
            .collect()
    }

    /// Reverse the winding of `facets`, so their normals point the other way.
    ///
    /// Returns how many were actually flipped. Position, normal and area are
    /// recomputed from the reordered geometry rather than the normal simply
    /// being negated, so the three stay mutually consistent whichever
    /// representation the mesh is in.
    ///
    /// Do this **before the window is created**: the GPU vertex buffers are
    /// built once in `Window::new`, so a flip applied afterwards would fix
    /// the physics and leave the renderer -- the shadow map and the hemicube
    /// among it -- still drawing the old winding.
    ///
    /// `_vertices_before_flatten` is not updated, so a later `smoothen` would
    /// undo this. Nothing in the run path does that.
    pub fn flip_facets(&mut self, facets: &[u32]) -> usize {
        let flat = self.is_flat();
        let mut flipped = 0;

        for &fi in facets {
            let f = fi as usize;
            if f >= self.facets.len() {
                continue;
            }
            // One or the other, never both: a flat mesh carries its winding
            // in the vertex order and its indices are the identity, so
            // swapping those too would undo the swap for everything that
            // reads through them.
            if flat && f * 3 + 2 < self.vertices.len() {
                self.vertices.swap(f * 3 + 1, f * 3 + 2);
            } else if f * 3 + 2 < self.indices.len() {
                self.indices.swap(f * 3 + 1, f * 3 + 2);
            }
            flipped += 1;
        }

        for &fi in facets {
            let f = fi as usize;
            if f >= self.facets.len() {
                continue;
            }
            let [a, b, c] = self.get_facet_positions(f).map(|p| *p);
            let (ab, ac) = (b - a, c - a);
            let normal = normal_facet(&ab, &ac);
            self.facets[f] = Facet {
                pos: (a + b + c) / 3.0,
                normal,
                area: area_facet(&ab, &ac),
            };
            if flat {
                for k in 0..3 {
                    if f * 3 + k < self.vertices.len() {
                        self.vertices[f * 3 + k].normal = normal;
                    }
                }
            }
        }

        flipped
    }

    pub fn recompute_facets(&mut self) {
        self.facets = compute_facets(&self.vertices, &self.indices);
    }

    pub fn is_flat(&self) -> bool {
        self.flat
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

    /// Where a ray meets one *named* facet, in the mesh's own frame.
    ///
    /// The counterpart to `intersect` for when the facet is already known: a
    /// GPU pick answers **which** facet in one texel, and this recovers
    /// **where** with a single triangle test, instead of the O(facets) sweep
    /// `intersect` needs to answer both at once.
    ///
    /// `None` when the ray misses -- which it can, marginally, even for the
    /// facet a pixel was rasterised from: the pixel centre and the ray through
    /// it are the same point only up to the rasteriser's fill rule, so a hit
    /// right on an edge can fall the other side of it.
    pub fn intersect_facet(&self, p: &Vec3, u: &Vec3, facet: usize) -> Option<Vec3> {
        let n = if self.is_flat() {
            self.vertices.len() / 3
        } else {
            self.indices.len() / 3
        };
        if facet >= n {
            return None;
        }
        let [a, b, c] = self.get_facet_positions(facet);
        intersect_triangle_moller_trumbore(p, u, a, b, c)
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

        // Streamed from the file, not read into a `String` first: the whole
        // text sat in memory for the length of the parse, 163 MB for a 3M
        // facet model, on top of tobj's own working set. Measured in
        // `notes/2026-09-18_memory_meshes_and_shadow_maps.md`.
        let file = std::fs::File::open(path)
            .unwrap_or_else(|e| panic!("cannot open mesh {}: {e}", path.display()));
        let mut obj_reader = std::io::BufReader::with_capacity(1 << 20, file);

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
                    // Dropped: no live shader samples a texture, and the
                    // normals a mesh renders with are computed from its
                    // facets below.
                    texcoords: _,
                    normals: _,
                    indices,
                    material_id,
                    ..
                } = mesh;

                // println!("MESH LOADING DEBUG");
                // println!("pos:{}", positions.len());
                // println!("indices:{}", indices.len());
                // println!("normals:{}", normals.len());

                // Texture coordinates and normals in the file are dropped: no
                // live shader samples a texture or uses a supplied normal, and
                // the normals a mesh renders with are computed below from the
                // facets. `Vertex` carries neither any more.
                let vertices: Vec<Vertex> = (0..positions.len() / 3)
                    .map(|i| Vertex {
                        pos: update_pos(
                            [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]].into(),
                        ),
                        ..Vertex::default()
                    })
                    .collect();
                let facets = compute_facets(&vertices, &indices);

                let bounds = Aabb::from_vertices(&vertices);

                let mut mesh = Mesh {
                    vertices,
                    indices,
                    facets,
                    material_id,

                    // temporary until better solution is found
                    _vertices_before_flatten: vec![],
                    _indices_before_flatten: vec![],
                    colors_dirty: false,
                    flat: false,
                    bounds,
                    path: Some(path.to_path_buf()),
                    values: vec![],
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

pub fn compute_facets(vertices: &[Vertex], indices: &[u32]) -> Vec<Facet> {
    let mut facets: Vec<Facet> = vec![];

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

    facets
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
#[cfg_attr(feature = "python", pyfunction)]
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
#[cfg_attr(feature = "python", pyfunction)]
pub fn view_factor_scalar(angle_at_a: Float, angle_at_b: Float, distance_a2b: Float) -> Float {
    view_factor_scalar_cos(angle_at_a.cos(), angle_at_b.cos(), distance_a2b)
}

/// The same kernel taking cosines directly.
///
/// Callers that have unit vectors already hold the cosines as dot products.
/// Going through an angle means `acos` followed by `cos`, which is two
/// transcendentals to recover a number that was in hand, and which loses
/// precision at small angles -- `acos` has unbounded derivative at 1, so the
/// round trip is worst exactly where facets face each other squarely and the
/// view factor is largest. Over an O(N^2) loop both costs matter.
pub fn view_factor_scalar_cos(cos_a: Float, cos_b: Float, distance_a2b: Float) -> Float {
    cos_a * cos_b / (crate::util::PI * distance_a2b.powi(2))
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
///
/// **This is the point-to-point approximation** and it is only valid while
/// the separation is large against the facets. It has no occlusion test and
/// no near-field treatment; see `view_factor_triangles` for the form that
/// handles both, and note 2026-08-27 section 9 for why that matters.
pub fn view_factor_facets(face_a: &Facet, face_b: &Facet, trans_b2a: &Mat4) -> Float {
    // Vector from center of facet **A** to facet **B**.
    let vector_a2b = (trans_b2a * face_b.pos.extend(1.0)).xyz() - face_a.pos;
    let distance_a2b = vector_a2b.length();
    let unit_a2b = vector_a2b.normalize();

    // This is a condition on the relation between distance of the two facets and their surface area to avoid too large
    // view factor in case of very close distance.
    //
    // Returning zero is wrong, not merely approximate: adjacent facets have a
    // centroid separation of order sqrt(area), which is exactly this
    // threshold, so the guard fires on the very neighbours that dominate
    // self-heating inside a concavity. `view_factor_triangles` subdivides
    // instead, and is what the thermophysical model should use.
    if distance_a2b < face_b.area.sqrt() {
        return 0.0;
    }

    // Cosines from both normals and the unit vector to the other facet. The
    // normal of facet B needs to be transformed to fixed-frame A.
    let cos_a = face_a.normal.dot(unit_a2b);
    let cos_b = trans_b2a.transform_vector3(face_b.normal).dot(-unit_a2b);

    // Another condition is one that was actually mentioned earlier: both
    // facets must face each other, i.e. angles below 90 degrees.
    if cos_a <= 0.0 || cos_b <= 0.0 {
        return 0.0;
    }

    view_factor_scalar_cos(cos_a, cos_b, distance_a2b)
}

/// A triangle in whatever frame the caller is working in.
pub type Triangle = [Vec3; 3];

fn tri_centroid(t: &Triangle) -> Vec3 {
    (t[0] + t[1] + t[2]) / 3.0
}

fn tri_normal_area(t: &Triangle) -> (Vec3, Float) {
    let cross = (t[1] - t[0]).cross(t[2] - t[0]);
    let len = cross.length();
    if len <= Float::EPSILON {
        return (Vec3::ZERO, 0.0);
    }
    (cross / len, 0.5 * len)
}

/// Split a triangle into four by joining its edge midpoints.
///
/// The middle triangle is inverted in winding but has the same plane and
/// area, and only the centroid, normal direction and area are used here, so
/// the orientation is recovered from the parent rather than from the
/// sub-triangle itself.
fn tri_subdivide(t: &Triangle) -> [Triangle; 4] {
    let m0 = (t[0] + t[1]) * 0.5;
    let m1 = (t[1] + t[2]) * 0.5;
    let m2 = (t[2] + t[0]) * 0.5;
    [
        [t[0], m0, m2],
        [m0, t[1], m1],
        [m2, m1, t[2]],
        [m0, m1, m2],
    ]
}

/// View factor `F(A->B)` between two triangles, subdividing when they are
/// close enough that the point-to-point form breaks down.
///
/// Returns the dimensionless fraction of energy leaving A that reaches B,
///
/// ```text
/// F = (1/A_a) * sum_i sum_j  cos_i cos_j / (pi d_ij^2) * a_i * a_j
/// ```
///
/// which is the double area integral evaluated on a uniform subdivision.
///
/// `ratio` is the separation, in units of the local facet size, below which a
/// pair is refined; 4 to 8 is the usual range and the error falls steeply
/// with it. `max_level` bounds the recursion, since two triangles sharing an
/// edge have sub-pairs at arbitrarily small separation and the integral,
/// while finite, is only reached in the limit.
///
/// No occlusion test: this is the near-field *geometry* fix. Visibility is
/// the hemicube's job.
pub fn view_factor_triangles(
    tri_a: &Triangle,
    tri_b: &Triangle,
    ratio: Float,
    max_level: u32,
) -> Float {
    let (_, area_a) = tri_normal_area(tri_a);
    if area_a <= 0.0 {
        return 0.0;
    }
    integrate_pair(tri_a, tri_b, ratio, max_level) / area_a
}

/// The unnormalised double integral, in units of area.
///
/// Kept separate from `view_factor_triangles` because the recursion sums
/// these directly; dividing by `A_a` at every level would be wrong, and
/// dividing at the end is both correct and cheaper.
fn integrate_pair(tri_a: &Triangle, tri_b: &Triangle, ratio: Float, level: u32) -> Float {
    let (n_a, area_a) = tri_normal_area(tri_a);
    let (n_b, area_b) = tri_normal_area(tri_b);
    if area_a <= 0.0 || area_b <= 0.0 {
        return 0.0;
    }

    let c_a = tri_centroid(tri_a);
    let c_b = tri_centroid(tri_b);
    let v = c_b - c_a;
    let d = v.length();
    if d <= Float::EPSILON {
        return 0.0;
    }

    // Refine while the pair is close relative to its own size. Using the
    // larger of the two areas is deliberate: a small facet next to a large
    // one still needs the large one split, and testing only `area_b` (as the
    // original guard did) misses that.
    let size = area_a.max(area_b).sqrt();
    if level > 0 && d < ratio * size {
        let mut total = 0.0;
        for sub_a in tri_subdivide(tri_a).iter() {
            for sub_b in tri_subdivide(tri_b).iter() {
                total += integrate_pair(sub_a, sub_b, ratio, level - 1);
            }
        }
        return total;
    }

    let u = v / d;
    let cos_a = n_a.dot(u);
    let cos_b = n_b.dot(-u);
    if cos_a <= 0.0 || cos_b <= 0.0 {
        return 0.0;
    }

    view_factor_scalar_cos(cos_a, cos_b, d) * area_a * area_b
}

/// Largest slope angle of spherical segment, in radian.
///
/// S: curvature diameter
#[allow(non_snake_case)]
#[cfg_attr(feature = "python", pyfunction)]
pub fn largest_slope_angle_sphere(S: Float) -> Float {
    (1.0 - 2.0 * S).acos()
}

/// Curvature diameter of spherical segment, in radian.
///
/// g: largest slope angle
#[allow(non_snake_case)]
#[cfg_attr(feature = "python", pyfunction)]
pub fn curvature_diameter_sphere(S: Float) -> Float {
    (1.0 - S.cos()) / 2.0
}

/// Curvature radius in a concave segment, in radian.
///
/// r: radius crater
/// d: depth crater
#[cfg_attr(feature = "python", pyfunction)]
pub fn curvature_radius(r: Float, d: Float) -> Float {
    (r.powi(2) + d.powi(2)) / (2.0 * d)
}

/// Curvature diameter from radius, in a concave segment, in radian.
///
/// R: curvature radius
/// d: depth crater
#[allow(non_snake_case)]
#[cfg_attr(feature = "python", pyfunction)]
pub fn curvature_diameter_from_radius(d: Float, R: Float) -> Float {
    d / (2.0 * R)
}

/// Z position inside crater
///
/// x, y: position
/// r: radius crater
/// d: depth crater
#[allow(non_snake_case)]
#[cfg_attr(feature = "python", pyfunction)]
pub fn z_in_crater(x: Float, y: Float, r: Float, d: Float) -> Float {
    let R = curvature_radius(r, d);
    R - d - (R.powi(2) - x.powi(2) - y.powi(2)).sqrt()
}

/// RMS slope, in radian
///
/// f: coverage
/// g: largest slope angle
#[cfg_attr(feature = "python", pyfunction)]
pub fn rms_slope(f: Float, g: Float) -> Float {
    (f / 2.0 * (g.powi(2) - (g * g.cos() - g.sin()).powi(2) / g.sin().powi(2))).sqrt()
}

/// RMS slope in case of hemispherical crater, in radian
///
/// f: coverage
#[cfg_attr(feature = "python", pyfunction)]
pub fn rms_slope_hemisphere(f: Float) -> Float {
    49.0 * f.sqrt()
}

/// RMS slope for a terrain, in radian
///
/// theta: angle between facet normal and average normal of terrain
/// a: facet area
pub fn rms_slope_terrain(
    theta: ndarray::ArrayView1<Float>,
    a: ndarray::ArrayView1<Float>,
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

#[cfg_attr(feature = "python", pyfunction)]
pub fn distribution_slope_angles(theta: Float, a: Float, b: Float) -> Float {
    a * (-theta.tan().powi(2) / b).exp() * theta.sin() / theta.cos().powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit square split along its diagonal: four shared corners, two
    /// facets. Small enough that "which row is which" is checkable by eye,
    /// and it has a shared edge, which is the whole point of flattening.
    fn square() -> Mesh {
        let mut m = Mesh::new();
        m.vertices = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ]
        .iter()
        .map(|p| Vertex {
            pos: (*p).into(),
            ..Vertex::default()
        })
        .collect();
        m.indices = vec![0, 1, 2, 0, 2, 3];
        m.facets = compute_facets(&m.vertices, &m.indices);
        m
    }

    #[test]
    fn flatten_renumbers_the_indices_to_address_the_rebuilt_vertices() {
        let mut m = square();
        let before: Vec<[Vec3; 3]> = (0..2).map(|f| m.get_facet_positions(f).map(|p| *p)).collect();

        m.flatten();

        assert!(m.is_flat());
        assert_eq!(m.vertices.len(), 6);
        assert_eq!(m.indices, vec![0, 1, 2, 3, 4, 5]);
        // The corners must still be where they were, read back through the
        // indices -- which is what left the shared ones in place broke.
        for f in 0..2 {
            assert_eq!(m.get_facet_positions(f).map(|p| *p), before[f], "facet {f}");
        }
    }

    #[test]
    fn recompute_facets_survives_a_flatten() {
        // The regression this renumbering exists for: `compute_facets` reads
        // `indices`, so stale ones sent a flattened mesh's normals to NaN off
        // a degenerate triangle.
        let mut m = square();
        let want: Vec<(Vec3, Float)> = m.facets.iter().map(|f| (f.normal, f.area)).collect();

        m.flatten();
        m.recompute_facets();

        for (f, (n, a)) in want.iter().enumerate() {
            assert!(
                (m.facets[f].normal - *n).length() < 1e-6,
                "facet {f} normal {:?} vs {n:?}",
                m.facets[f].normal
            );
            assert!((m.facets[f].area - a).abs() < 1e-6, "facet {f} area");
        }
        let total: Float = m.facets.iter().map(|f| f.area).sum();
        assert!((total - 1.0).abs() < 1e-6, "unit square, got {total}");
    }

    #[test]
    fn smoothen_puts_the_shared_topology_back() {
        let mut m = square();
        let want = m.indices.clone();
        m.flatten();
        m.smoothen();
        assert!(!m.is_flat());
        assert_eq!(m.indices, want);
        assert_eq!(m.vertices.len(), 4);
    }
}

/// Phase times of a load, printed on drop when `KALAST_TIMING` is set --
/// `[LOAD] read 31 ms | parse 92 ms | build 61 ms` -- because a load that
/// feels slow is one of four things and guessing which has been wrong twice.
struct LoadTimer {
    on: bool,
    last: std::time::Instant,
    phases: Vec<(&'static str, std::time::Duration)>,
}

impl LoadTimer {
    fn start() -> Self {
        Self {
            on: std::env::var_os("KALAST_TIMING").is_some(),
            last: std::time::Instant::now(),
            phases: Vec::new(),
        }
    }
    fn phase(&mut self, name: &'static str) {
        if self.on {
            let now = std::time::Instant::now();
            self.phases.push((name, now - self.last));
            self.last = now;
        }
    }
}

impl Drop for LoadTimer {
    fn drop(&mut self) {
        if self.on && !self.phases.is_empty() {
            let total: std::time::Duration = self.phases.iter().map(|p| p.1).sum();
            let parts: Vec<String> = self
                .phases
                .iter()
                .map(|(n, d)| format!("{n} {:.0} ms", d.as_secs_f64() * 1e3))
                .collect();
            eprintln!("[LOAD] {} | total {:.0} ms", parts.join(" | "), total.as_secs_f64() * 1e3);
        }
    }
}

/// Threads for the parallel parts of a load: the machine's, capped.
pub(crate) fn load_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, 16)
}

/// Parse an OBJ that is nothing but `v` and `f` lines -- comments, blank
/// lines, one `o`/`g` group and `s` lines allowed -- into positions and
/// triangles, in parallel over the bytes. `None` for anything else, which
/// is not a failure but a handover to tobj, whose semantics for texture
/// coordinates, normals, materials and several objects this does not
/// reproduce.
///
/// Faces are 1-based absolute indices; polygons are fanned `(0, i, i+1)`,
/// the way tobj triangulates them. Every index is checked against the
/// vertex count once the chunks are joined, since a face may name a vertex
/// defined further down the file.
pub(crate) fn parse_plain_obj(bytes: &[u8]) -> Option<(Vec<Vec3>, Vec<[u32; 3]>)> {
    // Chunk boundaries fall after a newline, so no line is split.
    let n = load_threads().min(bytes.len() / (1 << 16) + 1);
    let mut cuts = vec![0usize];
    for i in 1..n {
        let mut at = bytes.len() * i / n;
        while at < bytes.len() && bytes[at] != b'\n' {
            at += 1;
        }
        at = (at + 1).min(bytes.len());
        if at > *cuts.last().unwrap() {
            cuts.push(at);
        }
    }
    if *cuts.last().unwrap() < bytes.len() {
        cuts.push(bytes.len());
    }

    let parts: Vec<Option<(Vec<Vec3>, Vec<[u32; 3]>, usize)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = cuts
            .windows(2)
            .map(|w| {
                let chunk = &bytes[w[0]..w[1]];
                scope.spawn(move || parse_plain_chunk(chunk))
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut positions = Vec::new();
    let mut tris = Vec::new();
    let mut groups = 0;
    for part in parts {
        let (p, t, g) = part?;
        positions.extend(p);
        tris.extend(t);
        groups += g;
    }
    if groups > 1 || positions.is_empty() || tris.is_empty() {
        return None;
    }
    let n_v = positions.len() as u32;
    if tris.iter().any(|t| t.iter().any(|&i| i >= n_v)) {
        return None;
    }
    Some((positions, tris))
}

/// One chunk of lines: positions, triangles (0-based) and the number of
/// `o`/`g` lines seen. `None` bails the whole parse to tobj.
fn parse_plain_chunk(chunk: &[u8]) -> Option<(Vec<Vec3>, Vec<[u32; 3]>, usize)> {
    let mut positions = Vec::new();
    let mut tris = Vec::new();
    let mut groups = 0;
    for line in chunk.split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let line = line.trim_ascii_start();
        let Some(&head) = line.first() else {
            continue;
        };
        let rest = &line[1..];
        let blank_after = matches!(rest.first(), Some(b' ') | Some(b'\t'));
        match head {
            b'#' => {}
            b'v' if blank_after => {
                let text = std::str::from_utf8(rest).ok()?;
                let mut it = text.split_ascii_whitespace();
                let x: Float = it.next()?.parse().ok()?;
                let y: Float = it.next()?.parse().ok()?;
                let z: Float = it.next()?.parse().ok()?;
                positions.push(Vec3::new(x, y, z));
            }
            b'f' if blank_after => {
                let text = std::str::from_utf8(rest).ok()?;
                let mut corners: [u32; 8] = [0; 8];
                let mut k = 0;
                for tok in text.split_ascii_whitespace() {
                    if k == 8 || !tok.bytes().all(|b| b.is_ascii_digit()) {
                        return None; // slashes, negatives, or more than an octagon
                    }
                    let one_based: u32 = tok.parse().ok()?;
                    corners[k] = one_based.checked_sub(1)?;
                    k += 1;
                }
                if k < 3 {
                    return None;
                }
                for i in 1..k - 1 {
                    tris.push([corners[0], corners[i], corners[i + 1]]);
                }
            }
            b'o' | b'g' if blank_after || rest.is_empty() => groups += 1,
            b's' if blank_after => {}
            _ => return None, // vt, vn, vp, mtllib, usemtl, or anything unknown
        }
    }
    Some((positions, tris, groups))
}

/// The parsed file in tobj's order -- vertices numbered by first appearance
/// in the faces, unreferenced ones dropped -- which is what both builds take
/// and what the sidecar cache stores.
fn canonicalise(positions: &[Vec3], tris: &[[u32; 3]]) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let mut remap = vec![u32::MAX; positions.len()];
    let mut out_pos = Vec::new();
    let mut out_tris = Vec::with_capacity(tris.len());
    for t in tris {
        let mut n = [0u32; 3];
        for (k, &old) in t.iter().enumerate() {
            let slot = &mut remap[old as usize];
            if *slot == u32::MAX {
                *slot = out_pos.len() as u32;
                out_pos.push(positions[old as usize]);
            }
            n[k] = *slot;
        }
        out_tris.push(n);
    }
    (out_pos, out_tris)
}

/// Positions and triangles of a plain OBJ, canonical, or `None` to hand
/// over to tobj. Parsed every time: a cache beside the data was tried and
/// rejected -- kalast does not leave files next to a user's models.
fn plain_mesh(path: &std::path::Path, timer: &mut LoadTimer) -> Option<(Vec<Vec3>, Vec<[u32; 3]>)> {
    let bytes = std::fs::read(path).ok()?;
    timer.phase("read");
    let (positions, tris) = parse_plain_obj(&bytes)?;
    drop(bytes);
    timer.phase("parse");
    let out = canonicalise(&positions, &tris);
    timer.phase("canonicalise");
    Some(out)
}

/// The shared mesh of `tris` over `positions`, in tobj's order: vertices
/// numbered by first appearance in the faces, unreferenced ones dropped,
/// facets as `compute_facets` computes them, and a normal per vertex summed
/// from its facets in facet order and normalised -- the arithmetic of
/// `smoothen`, in its order, so the bits agree.
fn build_smooth(positions: &[Vec3], tris: &[[u32; 3]]) -> (Vec<Vertex>, Vec<u32>, Vec<Facet>) {
    // `positions` and `tris` are canonical -- see `canonicalise` -- so the
    // vertices are the positions as they come and the indices the triangles
    // flattened.
    let mut vertices: Vec<Vertex> = positions
        .iter()
        .map(|&pos| Vertex {
            pos,
            ..Vertex::default()
        })
        .collect();
    let indices: Vec<u32> = tris.iter().flatten().copied().collect();

    // Facets in parallel: each reads the shared vertices, writes its own.
    let n = indices.len() / 3;
    let mut facets = vec![Facet::default(); n];
    let step = n.div_ceil(load_threads()).max(1);
    std::thread::scope(|scope| {
        for (fs, is) in facets.chunks_mut(step).zip(indices.chunks(3 * step)) {
            let vertices = &vertices;
            scope.spawn(move || {
                for (f, fv) in fs.iter_mut().zip(is.chunks(3)) {
                    let a = vertices[fv[0] as usize].pos;
                    let b = vertices[fv[1] as usize].pos;
                    let c = vertices[fv[2] as usize].pos;
                    let ab = b - a;
                    let ac = c - a;
                    *f = Facet {
                        pos: (a + b + c) / 3.0,
                        normal: normal_facet(&ab, &ac),
                        area: area_facet(&ab, &ac),
                    };
                }
            });
        }
    });

    // Vertex normals: a scatter onto shared corners, so sequential, in the
    // order `smoothen` adds them.
    for (fi, fv) in indices.chunks(3).enumerate() {
        for &c in fv {
            vertices[c as usize].normal += facets[fi].normal;
        }
    }
    for v in vertices.iter_mut() {
        v.normal = v.normal.normalize();
    }
    (vertices, indices, facets)
}

/// The flat vertices and facets of `tris` over `positions`, built in
/// parallel: each triangle's three corners take its facet normal, exactly
/// as `flatten` gives them, and its centre, normal and area are computed in
/// the same pass, exactly as `compute_facets` does.
fn build_flat(positions: &[Vec3], tris: &[[u32; 3]]) -> (Vec<Vertex>, Vec<Facet>) {
    let n = tris.len();
    // Uninitialised, not `vec![default; n]`: the fill wrote the 684 MB of a
    // 3M-facet model once before the threads wrote it again, and was a third
    // of the build. Every element is written below: the chunks partition
    // both vectors exactly, in step with the triangles.
    let mut vertices: Vec<std::mem::MaybeUninit<Vertex>> = Vec::with_capacity(3 * n);
    let mut facets: Vec<std::mem::MaybeUninit<Facet>> = Vec::with_capacity(n);
    // SAFETY: `MaybeUninit` needs no initialisation; the capacity is there.
    unsafe {
        vertices.set_len(3 * n);
        facets.set_len(n);
    }
    let step = n.div_ceil(load_threads()).max(1);
    std::thread::scope(|scope| {
        for ((vs, fs), ts) in vertices
            .chunks_mut(3 * step)
            .zip(facets.chunks_mut(step))
            .zip(tris.chunks(step))
        {
            scope.spawn(move || {
                for ((v, f), t) in vs.chunks_mut(3).zip(fs.iter_mut()).zip(ts) {
                    let a = positions[t[0] as usize];
                    let b = positions[t[1] as usize];
                    let c = positions[t[2] as usize];
                    let ab = b - a;
                    let ac = c - a;
                    let normal = normal_facet(&ab, &ac);
                    f.write(Facet {
                        pos: (a + b + c) / 3.0,
                        normal,
                        area: area_facet(&ab, &ac),
                    });
                    for (slot, pos) in v.iter_mut().zip([a, b, c]) {
                        slot.write(Vertex {
                            pos,
                            normal,
                            ..Vertex::default()
                        });
                    }
                }
            });
        }
    });
    // SAFETY: every element was written above -- `3 * step` vertices and
    // `step` facets per chunk of `step` triangles, over all `n` triangles.
    let vertices = unsafe { assume_init_vec(vertices) };
    let facets = unsafe { assume_init_vec(facets) };
    (vertices, facets)
}

/// `Vec<MaybeUninit<T>>` to `Vec<T>` once every element is written.
///
/// # Safety
/// Every element must have been initialised.
unsafe fn assume_init_vec<T>(v: Vec<std::mem::MaybeUninit<T>>) -> Vec<T> {
    let mut v = std::mem::ManuallyDrop::new(v);
    // SAFETY: `MaybeUninit<T>` has the layout of `T`, and the caller vouches
    // for the contents; the allocation is handed over, not duplicated.
    unsafe { Vec::from_raw_parts(v.as_mut_ptr() as *mut T, v.len(), v.capacity()) }
}

#[cfg(test)]
mod load_flat_tests {
    use super::*;

    fn same_mesh(a: &Mesh, b: &Mesh) {
        assert_eq!(a.vertices.len(), b.vertices.len(), "vertex count");
        assert_eq!(a.indices, b.indices, "indices");
        assert_eq!(a.facets.len(), b.facets.len(), "facet count");
        for (i, (x, y)) in a.vertices.iter().zip(&b.vertices).enumerate() {
            assert_eq!(x.pos, y.pos, "position of vertex {i}");
            assert_eq!(x.normal, y.normal, "normal of vertex {i}");
            assert_eq!(x.color, y.color, "colour of vertex {i}");
        }
        for (i, (x, y)) in a.facets.iter().zip(&b.facets).enumerate() {
            assert_eq!(x.pos, y.pos, "centre of facet {i}");
            assert_eq!(x.normal, y.normal, "normal of facet {i}");
            assert_eq!(x.area, y.area, "area of facet {i}");
        }
        assert_eq!(a.bounds.min, b.bounds.min);
        assert_eq!(a.bounds.max, b.bounds.max);
    }

    /// The fast path has to be indistinguishable from parse-then-flatten,
    /// down to the bit: same corners, same normals, same facets, same box.
    #[test]
    fn load_flat_matches_load_then_flatten_bitwise() {
        for path in ["res/cube.obj", "res/ico3.obj", "res/plane_crater_1024-5000_h=0.437.obj"] {
            let bytes = std::fs::read(path).unwrap();
            assert!(parse_plain_obj(&bytes).is_some(), "{path} should take the fast path");
            let fast = Mesh::load_flat(path, |x| x);
            let mut slow = Mesh::load_via_tobj(path, |x| x);
            slow.flatten();
            same_mesh(&fast, &slow);
            assert!(fast.is_flat() && slow.is_flat());
            assert!(fast._vertices_before_flatten.is_empty(), "nothing kept to go back to");
        }
    }

    /// And the shared mesh too: tobj's vertex order, its normals, its facets.
    #[test]
    fn load_matches_tobj_bitwise() {
        for path in ["res/cube.obj", "res/ico3.obj", "res/plane_crater_1024-5000_h=0.437.obj"] {
            let fast = Mesh::load(path, |x| x);
            let slow = Mesh::load_via_tobj(path, |x| x);
            same_mesh(&fast, &slow);
            assert!(!fast.is_flat() && !slow.is_flat());
        }
    }

    /// Comments, blank lines, CRLF, a quad fanned as tobj fans it, one
    /// object line, a face naming a vertex defined later.
    #[test]
    fn plain_parser_handles_the_format_shape_models_use() {
        let text = b"# a comment\r\no thing\n\nv 0 0 0\nv 1 0 0\r\nv 1 1 0\nf 1 2 3 4\nv 0 1 0\ns off\n";
        let (p, t) = parse_plain_obj(text).expect("plain file");
        assert_eq!(p.len(), 4);
        assert_eq!(t, vec![[0, 1, 2], [0, 2, 3]]);
    }

    /// Everything the fast path does not reproduce hands over to tobj.
    #[test]
    fn plain_parser_bails_on_what_it_does_not_speak() {
        for text in [
            &b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nf 1 2 3\n"[..],
            b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1/1 2/2 3/3\n",
            b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf -3 -2 -1\n",
            b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 4\n",
            b"mtllib a.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
            b"o a\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\no b\nf 1 2 3\n",
        ] {
            assert!(parse_plain_obj(text).is_none(), "{:?}", std::str::from_utf8(text));
        }
    }

    /// A mesh loaded flat cannot go back: `smoothen` says so and leaves it flat.
    #[test]
    fn a_mesh_loaded_flat_stays_flat_when_asked_to_smoothen() {
        let mut m = Mesh::load_flat("res/cube.obj", |x| x);
        assert!(!m.smoothen());
        assert!(m.is_flat());
        assert_eq!(m.vertices.len(), 36);
        let mut shared = Mesh::load_via_tobj("res/cube.obj", |x| x);
        shared.flatten();
        assert!(shared.smoothen(), "an explicit flatten keeps its way back");
        assert!(!shared.is_flat());
    }
}
