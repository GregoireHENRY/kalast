use image::{GenericImageView, ImageBuffer, Rgba};
use wgpu::util::DeviceExt;

use crate::{Mat4, Vec3};

pub const SHADER_MAIN: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/shader_main.wgsl");

pub const SHADER_TRIANGLE: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/triangle.wgsl");

pub const SHADER_TRIANGLE_COLOR_XY: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/triangle_color_xy.wgsl");

pub const SHADER_VERTICES_COLOR: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/vertices_color.wgsl");

pub const SHADER_VERTICES_TEX: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/vertices_tex.wgsl");

pub const SHADER_MESH: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/mesh.wgsl");

pub const SHADER_MESH_SHADOW: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/mesh_shadow.wgsl");

pub const SHADER_DEPTH_RENDER: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/depth_render.wgsl");

pub const SHADER_COLORBAR: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/colorbar.wgsl");

pub const SHADER_AXES: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/axes.wgsl");

/// A `Mat4` as the column-major `[[f32; 4]; 4]` a uniform wants.
///
/// `Float` may be `f64`, and a uniform never is.
pub fn to_cols_f32(m: crate::Mat4) -> [[f32; 4]; 4] {
    let c = m.to_cols_array_2d();
    let mut out = [[0.0f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            out[i][j] = c[i][j] as f32;
        }
    }
    out
}

pub const SHADER_GRID: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/grid.wgsl");

pub const SHADER_GIZMO: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/gizmo.wgsl");

pub const SHADER_LIGHT_RENDER: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/light_render.wgsl");

pub const SHADER_SHADOW: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/shadow.wgsl");

pub const SHADER_FACET_SHADOW: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/facet_shadow.wgsl");

pub const SHADER_FACET_ID: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/facet_id.wgsl");

pub const SHADER_HEMICUBE: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/hemicube.wgsl");

pub const SHADER_HEMICUBE_ACCUMULATE: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/hemicube_accumulate.wgsl");

pub const SHADER_OCCLUSION: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/occlusion.wgsl");

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Reversed-Z: the camera's depth buffer holds 1.0 at the near plane and 0.0
/// at the far one, so its passes clear to 0.0 and keep the *greater* value.
///
/// Float precision bunches up near zero, and the perspective divide bunches it
/// near the near plane. Aiming the two at opposite ends makes them cancel, so
/// precision stops being a function of `near / far` -- which is what lets
/// `fit_projection` size the near plane from the scene instead of flooring it
/// to keep the ratio respectable.
///
/// Worthless on a fixed-point depth buffer, where the values are evenly spaced
/// and there is nothing to cancel. `DEPTH_FORMAT` is float, so it works here.
pub const DEPTH_COMPARE: wgpu::CompareFunction = wgpu::CompareFunction::Greater;
pub const DEPTH_CLEAR: f32 = 0.0;

/// The shadow map is deliberately **not** reversed: it keeps 0.0 at its near
/// plane and clears to 1.0.
///
/// Its projection is orthographic, so stored depth is linear in view-space z
/// and precision is already uniform over the range -- reversing it would buy
/// nothing. Against that, the biases in `mesh_shadow.wgsl` are calibrated
/// against this sense (`notes/2026-09-08_shadow_bias.md`) and
/// `facet_shadow.wgsl` re-derives the same comparison in compute, so flipping
/// it would move the illumination the thermophysical model runs on in exchange
/// for no precision at all.
pub const SHADOW_COMPARE: wgpu::CompareFunction = wgpu::CompareFunction::Less;
pub const SHADOW_CLEAR: f32 = 1.0;

pub struct Pipelines {
    pub main: RenderPipeline,
    pub more: Vec<RenderPipeline>,
}
pub struct RenderPipeline {
    pub inner: wgpu::RenderPipeline,
}

impl RenderPipeline {
    /// Opaque: the fragment colour replaces what is there.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        cull_mode: Option<wgpu::Face>,
        shader: wgpu::ShaderModuleDescriptor,
        bind_group_layouts: &[Option<&wgpu::BindGroupLayout>],
        depth_stencil: bool,
        fragment: bool,
        samples: u32,
        depth_write: bool,
        depth_compare: wgpu::CompareFunction,
        topology: wgpu::PrimitiveTopology,
        buffers: &[Option<wgpu::VertexBufferLayout>],
    ) -> Self {
        Self::build(
            device,
            format,
            cull_mode,
            shader,
            bind_group_layouts,
            depth_stencil,
            fragment,
            samples,
            depth_write,
            depth_compare,
            topology,
            buffers,
            wgpu::BlendState::REPLACE,
        )
    }

    /// Straight alpha over what is there.
    ///
    /// Only for a pass whose fragments are genuinely partly transparent --
    /// the ground grid, whose antialiasing *is* its alpha. Everything else
    /// here is opaque, and the shared constructor hardcoded `REPLACE` for
    /// long enough that the grid's alpha was silently thrown away: its lines
    /// came out flat, and where coverage saturated near the horizon the plane
    /// filled in solid instead of fading.
    #[allow(clippy::too_many_arguments)]
    pub fn blended(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        cull_mode: Option<wgpu::Face>,
        shader: wgpu::ShaderModuleDescriptor,
        bind_group_layouts: &[Option<&wgpu::BindGroupLayout>],
        depth_stencil: bool,
        fragment: bool,
        samples: u32,
        depth_write: bool,
        depth_compare: wgpu::CompareFunction,
        topology: wgpu::PrimitiveTopology,
        buffers: &[Option<wgpu::VertexBufferLayout>],
    ) -> Self {
        Self::build(
            device,
            format,
            cull_mode,
            shader,
            bind_group_layouts,
            depth_stencil,
            fragment,
            samples,
            depth_write,
            depth_compare,
            topology,
            buffers,
            wgpu::BlendState::ALPHA_BLENDING,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        cull_mode: Option<wgpu::Face>,
        shader: wgpu::ShaderModuleDescriptor,
        bind_group_layouts: &[Option<&wgpu::BindGroupLayout>],
        depth_stencil: bool,
        fragment: bool,
        samples: u32,
        // Whether this pipeline writes depth. Off for debug overlays: they
        // must be occluded *by* the scene without ever occluding it.
        depth_write: bool,
        // `DEPTH_COMPARE` for anything drawn through the camera, which is
        // reversed-Z, and `SHADOW_COMPARE` for the shadow map, which is not.
        // Must agree with the clear value the pass loads with, or the first
        // fragment tested against a cleared texel decides wrongly.
        depth_compare: wgpu::CompareFunction,
        // Lines for the axes, triangles for everything else. Line width is
        // always 1 px: WebGPU has no line width, so a thicker axis would have
        // to be built from triangles.
        topology: wgpu::PrimitiveTopology,
        // Vertex layouts. The axes carry their own -- position and colour,
        // no normals or instancing -- so this cannot be the mesh layout for
        // every pipeline.
        buffers: &[Option<wgpu::VertexBufferLayout>],
        blend: wgpu::BlendState,
    ) -> Self {
        // wireframe: bool,

        let shader = device.create_shader_module(shader);

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts,
            ..Default::default()
        });

        // let polygon_mode = wireframe
        //     .then(|| wgpu::PolygonMode::Line)
        //     .unwrap_or_else(|| wgpu::PolygonMode::Fill);

        let depth_stencil = (depth_stencil).then(|| wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(depth_write),
            depth_compare: Some(depth_compare),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState {
                // constant: 2, // bilinear filtering
                // slope_scale: 2.0,
                constant: 0, // bilinear filtering
                slope_scale: 0.0,
                clamp: 0.0,
            },
        });

        let fragment = if fragment {
            Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            })
        } else {
            None
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers,
                compilation_options: Default::default(),
            },
            fragment,
            primitive: wgpu::PrimitiveState {
                topology,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil,
            multisample: wgpu::MultisampleState {
                count: samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
            label: None,
        });

        Self { inner: pipeline }
    }
}

pub struct UniformBuffer<U: bytemuck::NoUninit> {
    pub uniform: U,
    pub buffer: wgpu::Buffer,
    pub layout: wgpu::BindGroupLayout,
}

impl<U: bytemuck::NoUninit> UniformBuffer<U> {
    pub fn new(device: &wgpu::Device, uniform: U) -> Self {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            label: None,
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
            label: None,
        });

        Self {
            uniform,
            buffer,
            layout,
        }
    }

    pub fn bind_group(&self, device: &wgpu::Device) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.buffer.as_entire_binding(),
            }],
            label: None,
        })
    }
}

// crate::mesh::Vertex interleaves static geometry (pos/tex/normal/tangent/
// bitangent) with attributes that some scripts mutate every frame (color/
// color_mode/extra, e.g. a per-facet colormap). Uploading that whole
// interleaved struct to the GPU on every change meant re-uploading
// geometry that never changes just because a color did. These two packed
// structs mirror the same fields split into two GPU buffers instead, so
// geometry can be uploaded once while attributes get updated independently
// -- see MeshBuffer's geometry_buffer/attrib_buffer.
//
// crate::mesh::Vertex itself (and the CPU-side Python API built on its
// interleaved layout, e.g. kalast.mesh.Mesh.positions/.colors) is
// unchanged; this split only affects how it's uploaded to the GPU.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GeometryVertex {
    pub pos: [f32; 3],
}

/// Everything about a surface that is not its position: read from a storage
/// buffer by the vertex stage, **one entry per facet** for a flat mesh and one
/// per vertex for a smooth one.
///
/// Per corner it was 20 bytes over 9.4 M corners on a 3M-facet model; per
/// facet it is 32 over 3.1 M. The normal moved here for the same reason: a
/// flat mesh's three corners carry one facet normal between them, so storing
/// it three times was storing it twice too often.
///
/// Laid out for std430 -- both `vec3`s land on a 16-byte boundary, so the
/// struct is exactly 32 bytes with no implicit padding. Keep it in step with
/// `MeshAttr` in `mesh_shadow.wgsl`.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshAttr {
    pub normal: [f32; 3],
    /// The facet's scalar, when the mesh carries `values`.
    pub value: f32,
    pub color: [f32; 3],
    pub color_mode: u32,
}

/// `crate::Vec3` is `f64` under `use_f64`; the GPU always wants `f32`.
fn f32x3(v: Vec3) -> [f32; 3] {
    [v.x as f32, v.y as f32, v.z as f32]
}

/// How many entries the attribute buffer holds: one per facet for a flat
/// mesh, one per vertex for a smooth one -- which is exactly what
/// `vertex_index / 3` and `vertex_index` index in the vertex shader.
pub fn n_attrs(mesh: &crate::mesh::Mesh) -> usize {
    if mesh.flat {
        mesh.facets.len()
    } else {
        mesh.positions.len()
    }
}

/// Which bind group the per-mesh attribute buffer occupies: after the five
/// the `Bindings` set, so nothing else moves.
pub const ATTRS_GROUP: u32 = 5;

/// The bind group layout the attribute buffer is bound through: one read-only
/// storage buffer, visible to the vertex stage.
pub fn mesh_attrs_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("mesh attrs"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

/// Vertices per upload slice. The GPU copies used to be built whole -- 56 +
/// 32 bytes a vertex, 830 MB for a 3M-facet model -- and then staged whole
/// again by `create_buffer_init`, so the first frame cost 2.3 GB of transient
/// RAM for the Didymos pair. Written in slices this size, the transient is one
/// slice: 23 MB. `notes/2026-09-18_memory_meshes_and_shadow_maps.md`.
const UPLOAD_CHUNK: usize = 1 << 18;

/// Fill `buffer` with `n` elements produced a slice at a time.
///
/// The slices of one batch are produced on all cores at once and then
/// written in order; the batch is one slice per core, so the transient stays
/// bounded (23 MB a slice) while the conversion -- which was a third of the
/// frame that uploads a 3M-facet mesh -- runs in parallel. The copies
/// themselves are memory-bound and stay sequential.
fn upload_chunked<T: bytemuck::Pod + Send>(
    queue: &wgpu::Queue,
    buffer: &wgpu::Buffer,
    n: usize,
    make: impl Fn(std::ops::Range<usize>) -> Vec<T> + Sync,
) {
    let stride = std::mem::size_of::<T>() as u64;
    let ranges: Vec<std::ops::Range<usize>> = (0..n)
        .step_by(UPLOAD_CHUNK)
        .map(|s| s..(s + UPLOAD_CHUNK).min(n))
        .collect();
    for batch in ranges.chunks(crate::mesh::load_threads()) {
        let slices: Vec<Vec<T>> = std::thread::scope(|scope| {
            let handles: Vec<_> = batch
                .iter()
                .map(|r| {
                    let (make, r) = (&make, r.clone());
                    scope.spawn(move || make(r))
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        for (r, slice) in batch.iter().zip(&slices) {
            queue.write_buffer(buffer, r.start as u64 * stride, bytemuck::cast_slice(slice));
        }
    }
}

/// An empty buffer of `n` elements of `T` (one at least: a zero-size buffer
/// is a validation error), to be filled by `upload_chunked`.
fn empty_buffer<T>(
    device: &wgpu::Device,
    n: usize,
    usage: wgpu::BufferUsages,
    label: &str,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (n.max(1) * std::mem::size_of::<T>()) as u64,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// The GPU's vertex positions for entries `range`.
///
/// A flat mesh is drawn non-indexed with one vertex per *corner*, so its
/// corners are expanded here -- `positions[indices[c]]` -- and nowhere on the
/// CPU, which is what lets the mesh keep its shared vertices. A smooth mesh
/// is drawn indexed and its positions go up as they are.
fn extract_geometry(mesh: &crate::mesh::Mesh, range: std::ops::Range<usize>) -> Vec<GeometryVertex> {
    if mesh.flat {
        range
            .map(|c| GeometryVertex {
                pos: f32x3(mesh.positions[mesh.indices[c] as usize]),
            })
            .collect()
    } else {
        extract_positions(mesh, range)
    }
}

/// The shared vertices `range`, as loaded: a smooth mesh's geometry, and a
/// flat mesh's `shared_positions`.
fn extract_positions(mesh: &crate::mesh::Mesh, range: std::ops::Range<usize>) -> Vec<GeometryVertex> {
    range
        .map(|v| GeometryVertex {
            pos: f32x3(mesh.positions[v]),
        })
        .collect()
}

/// How many vertices the GPU draws: one per corner for a flat mesh, one per
/// shared vertex for a smooth one.
pub fn n_gpu_vertices(mesh: &crate::mesh::Mesh) -> usize {
    if mesh.flat {
        mesh.indices.len()
    } else {
        mesh.positions.len()
    }
}

/// The attributes of entries `range` of the buffer: facets of a flat mesh,
/// vertices of a smooth one.
///
/// A flat facet takes its first corner's colour and mode. All three carry the
/// same one wherever anything in kalast writes them -- the selection, a
/// colormap, `update_all_vertices_colors` -- and three different colours on
/// one flat-shaded facet was never something flat shading could express.
///
/// `values` is per facet, so only a flat mesh has one to read; `set_values`
/// refuses an indexed mesh for that reason, and a smooth one gets zero.
fn extract_attrs(mesh: &crate::mesh::Mesh, range: std::ops::Range<usize>) -> Vec<MeshAttr> {
    let default = crate::mesh::Attr::default();
    range
        .map(|i| {
            let a = mesh.attrs.get(i).copied().unwrap_or(default);
            MeshAttr {
                normal: f32x3(if mesh.flat {
                    mesh.facets
                        .get(i)
                        .map(|f| f.normal)
                        .unwrap_or(crate::Vec3::ZERO)
                } else {
                    mesh.normals.get(i).copied().unwrap_or(crate::Vec3::ZERO)
                }),
                value: if mesh.flat {
                    mesh.values.get(i).copied().unwrap_or(0.0) as f32
                } else {
                    0.0
                },
                color: f32x3(a.color),
                color_mode: a.color_mode,
            }
        })
        .collect()
}

impl crate::mesh::Vertex {
    /// The position, and nothing else. Everything else a surface has is per
    /// facet and comes from the storage buffer -- see `MeshAttr`.
    pub const GEOMETRY_ATTRIBS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![
        0 => Float32x3,
    ];

    pub fn geometry_desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GeometryVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::GEOMETRY_ATTRIBS,
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceInput {
    pub mat: Mat4,

    normal: Mat4,
    //

    // Per-mesh flags. Bit 0 marks the mesh as flat (one vertex per triangle
    // corner, drawn non-indexed), which is what lets the shader recover
    // barycentric coordinates from vertex_index for the wireframe. Indexed
    // meshes share vertices, so the shader must skip wireframing them rather
    // than draw nonsense -- hence a per-mesh flag and not a global uniform.
    pub flags: u32,

    /// Which shadow layer shades this body. Each body's layer is aimed at
    /// and sized to that body, so the fragment shader must sample its own
    /// rather than a shared map.
    pub shadow_layer: u32,

    _padding: [u32; 2],
    // instance color used in vertex if color mode is 1
    // pub color: Vec3,

    // Control vertex instance color
    // - 0: vertex color
    // - 1: instance color
    // pub color_mode: u32,
}

pub const INSTANCE_FLAG_FLAT: u32 = 1;

/// The mesh carries per-facet `values`, so `color_mode == 1` shows the data
/// map rather than the vertex colours.
///
/// Needed because an empty `values` array uploads as zeros, which the shader
/// cannot tell from real zeros.
pub const INSTANCE_FLAG_HAS_VALUES: u32 = 2;

impl Default for InstanceInput {
    fn default() -> Self {
        Self {
            mat: Mat4::IDENTITY,
            normal: Mat4::IDENTITY,
            flags: 0,
            shadow_layer: 0,
            _padding: [0; 2],
            // color: Vec3::new(1.0, 1.0, 1.0),
            // color_mode: 0,
        }
    }
}

impl InstanceInput {
    pub fn new(mat: Mat4) -> Self {
        Self::new_with_flags(mat, 0)
    }

    pub fn new_with_flags(mat: Mat4, flags: u32) -> Self {
        Self::new_with_layer(mat, flags, 0)
    }

    pub fn new_with_layer(mat: Mat4, flags: u32, shadow_layer: u32) -> Self {
        let mut instance = Self {
            mat,
            flags,
            shadow_layer,
            ..Default::default()
        };
        instance.compute_normal();
        instance
    }

    pub fn compute_normal(&mut self) {
        self.normal = self.mat.inverse().transpose();
    }
}

pub struct MeshBuffer {
    pub n_vertices: u32,
    pub n_indices: u32,
    pub is_flat: bool,

    // Static: uploaded once in `new`, never re-uploaded -- no script
    // mutates vertex positions/normals/tex at runtime.
    pub geometry_buffer: wgpu::Buffer,
    // Dynamic: some scripts recolor every frame (e.g. a per-facet
    // colormap). Persistent buffer, updated in place via write_buffer
    // rather than reallocated -- see update_attrib_buffer. A storage
    // buffer, one entry per facet (flat) or per vertex (smooth), read by
    // the vertex stage through `attr_bind_group`.
    pub attrib_buffer: wgpu::Buffer,
    pub attr_bind_group: wgpu::BindGroup,
    pub index_buffer: wgpu::Buffer,
    // A flat mesh's shared vertices -- `positions` as loaded -- for the
    // passes that draw depth alone. `geometry_buffer` holds the corners
    // expanded, three per facet, because the shaded pass finds a facet's
    // attributes by `vertex_index / 3`; a depth-only pass has no attributes
    // to find and draws the shared vertices through `index_buffer` instead,
    // which shades 1.6 M vertices per pass on a 3M-facet model rather than
    // 9.4 M corners. `None` for a smooth mesh, whose `geometry_buffer`
    // already is the shared vertices.
    pub shared_positions: Option<wgpu::Buffer>,

    // Dynamic: the body's transform, changes every frame it moves.
    // Persistent buffer, updated in place via write_buffer.
    pub instance_buffer: wgpu::Buffer,
}

impl MeshBuffer {
    // matrix model
    pub const ATTRIBS: [wgpu::VertexAttribute; 10] = wgpu::vertex_attr_array![
        8  => Float32x4,
        9  => Float32x4,
        10 => Float32x4,
        11 => Float32x4,
        12  => Float32x4,
        13  => Float32x4,
        14 => Float32x4,
        15 => Float32x4,
        16 => Uint32,
        17 => Uint32,
        // 17 => Uint32,
    ];

    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        attrs_layout: &wgpu::BindGroupLayout,
        mesh: &crate::mesh::Mesh,
        instance: &InstanceInput,
    ) -> Self {
        let n = n_gpu_vertices(mesh);
        // STORAGE so the per-facet shadow query can read exactly the
        // geometry that gets drawn, rather than a second upload that could
        // drift out of sync. Filled a slice at a time; see `UPLOAD_CHUNK`
        // for why not `create_buffer_init`.
        let geometry_buffer = empty_buffer::<GeometryVertex>(
            device,
            n,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE,
            "mesh geometry",
        );
        upload_chunked(queue, &geometry_buffer, n, |r| extract_geometry(mesh, r));

        let shared_positions = mesh.flat.then(|| {
            let nv = mesh.positions.len();
            let buffer = empty_buffer::<GeometryVertex>(
                device,
                nv,
                wgpu::BufferUsages::VERTEX,
                "mesh shared positions",
            );
            upload_chunked(queue, &buffer, nv, |r| extract_positions(mesh, r));
            buffer
        });

        let na = n_attrs(mesh);
        let attrib_buffer =
            empty_buffer::<MeshAttr>(device, na, wgpu::BufferUsages::STORAGE, "mesh attributes");
        upload_chunked(queue, &attrib_buffer, na, |r| extract_attrs(mesh, r));

        Self::with_vertex_buffers(
            device,
            attrs_layout,
            geometry_buffer,
            attrib_buffer,
            shared_positions,
            n,
            &mesh.indices,
            instance,
            mesh.flat,
        )
    }

    /// For a mesh built into the program -- the light cube, the depth pass's
    /// quad -- where staging it whole is a few hundred bytes and no queue is
    /// to hand. Drawn indexed and never shaded, so its attributes are one
    /// default per position. Loaded meshes go through `new`.
    pub fn new_static(
        device: &wgpu::Device,
        attrs_layout: &wgpu::BindGroupLayout,
        positions: &[crate::Vec3],
        indices: &[u32],
        instance: &InstanceInput,
    ) -> Self {
        let geometry: Vec<GeometryVertex> = positions
            .iter()
            .map(|p| GeometryVertex { pos: f32x3(*p) })
            .collect();
        let geometry_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::cast_slice(&geometry),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE,
            label: Some("mesh geometry"),
        });
        let attrs = vec![
            MeshAttr {
                normal: [0.0; 3],
                value: 0.0,
                color: [1.0; 3],
                color_mode: 0,
            };
            positions.len().max(1)
        ];
        let attrib_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::cast_slice(&attrs),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            label: Some("mesh attributes"),
        });
        Self::with_vertex_buffers(
            device,
            attrs_layout,
            geometry_buffer,
            attrib_buffer,
            None,
            positions.len(),
            indices,
            instance,
            false,
        )
    }

    fn with_vertex_buffers(
        device: &wgpu::Device,
        attrs_layout: &wgpu::BindGroupLayout,
        geometry_buffer: wgpu::Buffer,
        attrib_buffer: wgpu::Buffer,
        shared_positions: Option<wgpu::Buffer>,
        n_vertices: usize,
        indices: &[u32],
        instance: &InstanceInput,
        is_flat: bool,
    ) -> Self {
        let attr_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mesh attrs"),
            layout: attrs_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: attrib_buffer.as_entire_binding(),
            }],
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::STORAGE,
            label: None,
        });

        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::bytes_of(instance),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            label: None,
        });

        Self {
            n_vertices: n_vertices as _,
            n_indices: indices.len() as _,
            is_flat,

            geometry_buffer,
            attrib_buffer,
            attr_bind_group,
            index_buffer,
            shared_positions,

            instance_buffer,
        }
    }

    /// Facet count as the compute pass indexes them: a flat mesh is
    /// triangle-major so every 3 vertices are one facet, an indexed one
    /// uses every 3 indices.
    pub fn n_facets(&self) -> u32 {
        if self.is_flat {
            self.n_vertices / 3
        } else {
            self.n_indices / 3
        }
    }

    // pub fn n_indices(&self) -> u32 {
    //     (self.index_buffer.size() / 4) as _
    // }

    /// Re-uploads normal/colour/mode/value, in place (no reallocation). Only
    /// call when they've actually changed -- e.g. a per-facet colormap script
    /// sets Mesh::colors_dirty after mutating `mesh.colors`, and
    /// Window::update checks that flag before calling this, so a
    /// static-coloured mesh never pays this cost after its initial upload in
    /// `new`.
    pub fn update_attrib_buffer(&mut self, queue: &wgpu::Queue, mesh: &crate::mesh::Mesh) {
        let n = n_attrs(mesh);
        upload_chunked(queue, &self.attrib_buffer, n, |r| extract_attrs(mesh, r));
    }

    /// Re-uploads the instance transform in place (no reallocation) --
    /// cheap (64 bytes), safe to call unconditionally every frame a body
    /// might have moved.
    pub fn update_instance_buffer(&mut self, queue: &wgpu::Queue, instance: &InstanceInput) {
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::bytes_of(instance));
    }

    /// Draw it with positions alone -- what the facet-id, hemicube and
    /// light-cube passes want, none of which shade. A flat mesh goes
    /// non-indexed, one vertex per corner, because those passes still name
    /// the facet by `vertex_index / 3`; a pass that wants depth and nothing
    /// else should call `render_depth`.
    pub fn render(&self, pass: &mut wgpu::RenderPass) {
        pass.set_vertex_buffer(0, self.geometry_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);

        if self.is_flat {
            pass.draw(0..self.n_vertices, 0..1);
        } else {
            pass.draw_indexed(0..self.n_indices, 0, 0..1);
        }
    }

    /// Draw it for its depth alone -- the shadow pass. Same triangles as
    /// `render`, but a flat mesh is drawn indexed over its shared vertices
    /// (`shared_positions`), so the vertex stage runs once per vertex rather
    /// than once per corner. Nothing downstream of a depth-only pass reads
    /// `vertex_index`, which is what makes the indexed draw equivalent.
    pub fn render_depth(&self, pass: &mut wgpu::RenderPass) {
        let Some(shared) = &self.shared_positions else {
            return self.render(pass);
        };
        pass.set_vertex_buffer(0, shared.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.n_indices, 0, 0..1);
    }

    /// Draw it shaded: the same, plus the attribute buffer at group
    /// `ATTRS_GROUP`. Only the main pass's pipeline declares that group, so
    /// only the main pass may call this.
    pub fn render_shaded(&self, pass: &mut wgpu::RenderPass) {
        pass.set_bind_group(ATTRS_GROUP, Some(&self.attr_bind_group), &[]);
        self.render(pass);
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceInput>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub struct Texture {
    pub inner: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub layout: Option<wgpu::BindGroupLayout>,
    /// One single-layer view per array layer, for rendering *into* a layer.
    /// `view` is the whole array and is what the main pass samples; a render
    /// pass cannot target an array view, so it needs these. Empty for
    /// non-layered textures.
    pub layer_views: Vec<wgpu::TextureView>,
}

impl Texture {
    pub fn new_image_from_bytes(device: &wgpu::Device, queue: &wgpu::Queue, bytes: &[u8]) -> Self {
        let image = image::load_from_memory(bytes).unwrap();
        let rgba = image.to_rgba8();
        let dimensions = image.dimensions();
        let size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };

        // Bgra8Unorm to see the exact color you ask
        // Rgba8UnormSrgb to have physically correct lighting
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            // format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
            label: None,
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * dimensions.0),
                rows_per_image: Some(dimensions.1),
            },
            size,
        );

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
            label: None,
        });

        Self {
            inner: texture,
            view,
            sampler,
            layout: Some(layout),
            layer_views: vec![],
        }
    }

    pub fn load_image<P: AsRef<std::path::Path>>(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: P,
    ) -> Self {
        Self::new_image_from_bytes(device, queue, &std::fs::read(path.as_ref()).unwrap())
    }

    /// A layered shadow map: one depth layer per body.
    ///
    /// One shared map has to be fitted to the whole scene, so a small body
    /// beside a large one gets almost no texels -- 6 km Deimos next to
    /// 3,396 km Mars is the case that forced this. A layer each lets every
    /// body's map be aimed at it and sized to it.
    ///
    /// The array view is what the main pass samples; `layer_views` are what
    /// the shadow pass renders into, since a render pass cannot target an
    /// array view.
    pub fn create_depth_texture_shadow_pass(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        layers: u32,
    ) -> Self {
        let layers = layers.max(1);
        let mut texture = Self::create_depth_texture_layered(
            device,
            width,
            height,
            layers,
            wgpu::FilterMode::Linear,
            Some(wgpu::CompareFunction::LessEqual),
        );

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    count: None,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                    },
                    visibility: wgpu::ShaderStages::FRAGMENT,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    count: None,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    visibility: wgpu::ShaderStages::FRAGMENT,
                },
            ],
            label: None,
        });
        texture.layout = Some(layout);

        texture
    }

    pub fn create_depth_texture_shadow_debug(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Self {
        let mut texture =
            Self::create_depth_texture(device, width, height, wgpu::FilterMode::Nearest, None);

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    count: None,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    visibility: wgpu::ShaderStages::FRAGMENT,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    count: None,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    visibility: wgpu::ShaderStages::FRAGMENT,
                },
            ],
            label: None,
        });
        texture.layout = Some(layout);

        texture
    }

    pub fn create_depth_texture_render_debug(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Self {
        let mut texture =
            Self::create_depth_texture(device, width, height, wgpu::FilterMode::Nearest, None);

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    count: None,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    visibility: wgpu::ShaderStages::FRAGMENT,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    count: None,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    visibility: wgpu::ShaderStages::FRAGMENT,
                },
            ],
            label: None,
        });
        texture.layout = Some(layout);

        texture
    }

    /// As `create_depth_texture`, with `layers` array layers, an array view
    /// for sampling and one single-layer view per layer for rendering into.
    pub fn create_depth_texture_layered(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        layers: u32,
        mag_min_filter: wgpu::FilterMode,
        compare: Option<wgpu::CompareFunction>,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
            label: Some("shadow array"),
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let layer_views = (0..layers)
            .map(|i| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: i,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: mag_min_filter,
            min_filter: mag_min_filter,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            compare,
            lod_min_clamp: 0.0,
            lod_max_clamp: 100.0,
            ..Default::default()
        });

        Self {
            inner: texture,
            view,
            sampler,
            layout: None,
            layer_views,
        }
    }

    pub fn create_depth_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        mag_min_filter: wgpu::FilterMode,
        compare: Option<wgpu::CompareFunction>,
    ) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
            label: None,
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: mag_min_filter,
            min_filter: mag_min_filter,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            compare,
            lod_min_clamp: 0.0,
            lod_max_clamp: 100.0,
            ..Default::default()
        });

        Self {
            inner: texture,
            view,
            sampler,
            layout: None,
            layer_views: vec![],
        }
    }

    pub fn bind_group(&self, device: &wgpu::Device) -> Option<wgpu::BindGroup> {
        self.layout.as_ref().map(|layout| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
                label: None,
            })
        })
    }
}

pub fn color_vec3(c: &wgpu::Color) -> Vec3 {
    Vec3::new(c.r as _, c.g as _, c.b as _)
}

pub fn linear_to_srgb_u8(x: u8) -> u8 {
    let f = (x as f32) / 255.0;
    let srgb = f.powf(1.0 / 2.2);
    (srgb * 255.0).min(255.0) as u8
}

struct PooledBuffer {
    buffer: wgpu::Buffer,
    size: wgpu::BufferAddress,
}

struct InFlightExport {
    buffer: wgpu::Buffer,
    buffer_size: wgpu::BufferAddress,
    ready: std::sync::Arc<std::sync::atomic::AtomicBool>,
    padded_bytes_per_row: u32,
    unpadded_bytes_per_row: u32,
    width: u32,
    height: u32,
    bgra_swap: bool,
    path: std::path::PathBuf,
}

struct SaveJob {
    buffer: wgpu::Buffer,
    buffer_size: wgpu::BufferAddress,
    pool_tx: std::sync::mpsc::Sender<PooledBuffer>,
    padded_bytes_per_row: u32,
    unpadded_bytes_per_row: u32,
    width: u32,
    height: u32,
    bgra_swap: bool,
    path: std::path::PathBuf,
}

/// Unpads rows, fixes channel order, PNG-encodes and writes one already-
/// mapped frame, then returns its buffer to the pool. Shared by the async
/// worker pool and the synchronous export path so both produce byte-for-
/// byte identical files.
fn save_job(job: SaveJob) {
    let data = job.buffer.slice(..).get_mapped_range().unwrap();

    let mut pixels = vec![0u8; (job.width * job.height * 4) as usize];

    for y in 0..job.height as usize {
        let src_offset = y * job.padded_bytes_per_row as usize;
        let dst_offset = y * job.unpadded_bytes_per_row as usize;
        pixels[dst_offset..dst_offset + job.unpadded_bytes_per_row as usize]
            .copy_from_slice(&data[src_offset..src_offset + job.unpadded_bytes_per_row as usize]);
    }

    if job.bgra_swap {
        for i in (0..pixels.len()).step_by(4) {
            pixels.swap(i, i + 2);
        }
    }

    drop(data);
    job.buffer.unmap();

    if let Some(img) = ImageBuffer::<Rgba<u8>, _>::from_raw(job.width, job.height, pixels) {
        if let Err(e) = img.save(&job.path) {
            eprintln!("[EXPORT] failed to save {:?}: {e}", job.path);
        }
    }

    let _ = job.pool_tx.send(PooledBuffer {
        buffer: job.buffer,
        size: job.buffer_size,
    });
}

/// Exports render-target frames to PNG (`{export_dir}/000000.png`) without
/// blocking the render loop.
///
/// The old `export_frame` free function did a full GPU pipeline stall
/// every call (`device.poll(PollType::Wait { timeout: None })`) followed by
/// synchronous PNG encoding + disk I/O on the render thread -- ~25x slower
/// than rendering with export disabled. This spreads the GPU->CPU copy
/// across frames (drained non-blockingly by `poll`) and moves the
/// encode/write work to a background thread, so `export_frame` only ever
/// costs a buffer copy + a cheap ownership handoff on the render thread.
pub struct FrameExporter {
    export_dir: std::path::PathBuf,
    in_flight: Vec<InFlightExport>,
    pool_tx: std::sync::mpsc::Sender<PooledBuffer>,
    pool_rx: std::sync::mpsc::Receiver<PooledBuffer>,
    // Option so `finish` can drop the sender to let the workers' receive
    // loop end, while still holding the rest of `self` (e.g. to log).
    save_tx: Option<std::sync::mpsc::Sender<SaveJob>>,
    save_workers: Vec<std::thread::JoinHandle<()>>,
    // Queued-or-encoding job count, for finish()'s progress message.
    pending: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    next_index: usize,
    // When set, export_frame blocks until the frame is on disk instead of
    // queueing it -- see Config::export_sync.
    sync: bool,
    // Max frames outstanding before export_frame blocks; 0 = unbounded.
    // See Config::export_max_queued.
    max_queued: usize,
}

impl FrameExporter {
    /// `export_dir` is where frames get written, as
    /// "{export_dir}/{N:06}.png". The resume scan below parses the stem as a
    /// number, which accepts both the padded form and the unpadded names
    /// earlier runs wrote, so an existing directory still resumes correctly.
    /// Give test/dev instances their own directory (e.g. a scratch path)
    /// distinct from a real run's -- two FrameExporters writing into the
    /// same directory concurrently will race on both the initial index
    /// scan and any cleanup (`rm -rf` of one process's directory can
    /// delete files a concurrently-running process just wrote).
    ///
    /// `sync` trades throughput for bounded memory -- see `Config::export_sync`.
    /// The worker pool is still spawned when set, so the mode is only read
    /// per-export and the two paths share all their machinery.
    ///
    /// `max_queued` bounds the async path's backlog -- see
    /// `Config::export_max_queued`. Ignored when `sync` is set, which already
    /// bounds the backlog to nothing.
    pub fn new(export_dir: impl Into<std::path::PathBuf>, sync: bool, max_queued: usize) -> Self {
        let export_dir = export_dir.into();
        std::fs::create_dir_all(&export_dir).unwrap();

        // Resume numbering after files already in export_dir (matches the
        // old behavior of never overwriting a previous run's frames),
        // scanned once here instead of on every export.
        let mut next_index = 0usize;
        if let Ok(entries) = std::fs::read_dir(&export_dir) {
            for entry in entries.flatten() {
                if let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()) {
                    if let Ok(n) = stem.parse::<usize>() {
                        next_index = next_index.max(n + 1);
                    }
                }
            }
        }

        let (pool_tx, pool_rx) = std::sync::mpsc::channel::<PooledBuffer>();
        let (save_tx, save_rx) = std::sync::mpsc::channel::<SaveJob>();
        let pending = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        // Pool of worker threads sharing one receiver: each reads a
        // (already-mapped) buffer, unpads rows, swaps channels if needed,
        // PNG-encodes and writes to disk -- all off the render thread, and
        // in parallel with each other, since encode+write for one frame was
        // otherwise the throughput ceiling for the whole pipeline. The
        // buffer is unmapped and handed back through pool_tx once done, for
        // reuse by future exports.
        let save_rx = std::sync::Arc::new(std::sync::Mutex::new(save_rx));
        let n_workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(2, 8);

        let save_workers = (0..n_workers)
            .map(|_| {
                let save_rx = save_rx.clone();
                let pending = pending.clone();

                std::thread::spawn(move || {
                    loop {
                        // Only hold the lock long enough to pop a job, so
                        // workers don't serialize on each other while
                        // encoding/writing.
                        let job = {
                            let rx = save_rx.lock().unwrap();
                            rx.recv()
                        };
                        let Ok(job) = job else { break };

                        save_job(job);

                        pending.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
                    }
                })
            })
            .collect();

        Self {
            export_dir,
            in_flight: Vec::new(),
            pool_tx,
            pool_rx,
            save_tx: Some(save_tx),
            save_workers,
            pending,
            next_index,
            sync,
            max_queued,
        }
    }

    /// Moves any in_flight entries whose GPU->CPU copy has completed onto
    /// the save queue. Returns false if there's no sender left to send to
    /// (i.e. `finish` already ran).
    fn drain_ready(&mut self) -> bool {
        let Some(save_tx) = self.save_tx.as_ref() else {
            return false;
        };

        let mut ii = 0;
        while ii < self.in_flight.len() {
            if self.in_flight[ii]
                .ready
                .load(std::sync::atomic::Ordering::Acquire)
            {
                let job = self.in_flight.remove(ii);
                let _ = save_tx.send(SaveJob {
                    buffer: job.buffer,
                    buffer_size: job.buffer_size,
                    pool_tx: self.pool_tx.clone(),
                    padded_bytes_per_row: job.padded_bytes_per_row,
                    unpadded_bytes_per_row: job.unpadded_bytes_per_row,
                    width: job.width,
                    height: job.height,
                    bgra_swap: job.bgra_swap,
                    path: job.path,
                });
                self.pending
                    .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            } else {
                ii += 1;
            }
        }
        true
    }

    /// Non-blocking: drains any GPU->CPU copies that have completed since
    /// the last call and hands them off to the save worker pool. Call once
    /// per frame regardless of whether an export was requested this frame,
    /// so in-flight exports from previous frames keep progressing.
    pub fn poll(&mut self, device: &wgpu::Device) {
        device.poll(wgpu::PollType::Poll).unwrap();
        self.drain_ready();
    }

    /// Blocks until every queued export has been saved to disk: first
    /// force-completes any GPU->CPU copies still pending (blocking, since
    /// this is only meant to be called once, at shutdown), then drops the
    /// job sender (so each worker's receive loop ends once the queue is
    /// drained) and joins all workers. Call this before the app actually
    /// exits (e.g. on window close), otherwise any export still queued,
    /// in-flight, or mid-encode when the process dies is silently lost --
    /// the OS kills the worker threads along with it, there is nothing to
    /// resume.
    pub fn finish(&mut self, device: &wgpu::Device) {
        while !self.in_flight.is_empty() {
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                })
                .unwrap();
            self.drain_ready();
        }

        let n = self.pending.load(std::sync::atomic::Ordering::Acquire);
        if n > 0 {
            println!("[EXPORT] waiting for {n} pending frame export(s) to finish saving...");
        }

        self.save_tx.take();
        for handle in self.save_workers.drain(..) {
            let _ = handle.join();
        }
    }

    /// Queues a non-blocking export of `texture`. The GPU->CPU copy
    /// completes over the following frames (drained by `poll`), and PNG
    /// encoding + the disk write happen on a background thread -- this
    /// call never blocks the render loop.
    pub fn export_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
    ) {
        let bytes_per_pixel = 4;
        let unpadded_bytes_per_row = bytes_per_pixel * width;
        let padding = (256 - unpadded_bytes_per_row % 256) % 256;
        let padded_bytes_per_row = unpadded_bytes_per_row + padding;
        let buffer_size = (padded_bytes_per_row * height) as wgpu::BufferAddress;

        // Reuse a pooled buffer of the right size if one's available
        // (stale sizes, e.g. after a resize, are just dropped), else
        // allocate a new one.
        let buffer = loop {
            match self.pool_rx.try_recv() {
                Ok(pooled) if pooled.size == buffer_size => break pooled.buffer,
                Ok(_) => continue,
                Err(_) => {
                    break device.create_buffer(&wgpu::BufferDescriptor {
                        size: buffer_size,
                        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                        mapped_at_creation: false,
                        label: None,
                    });
                }
            }
        };

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        queue.submit(Some(encoder.finish()));

        let bgra_swap = matches!(
            texture.format(),
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        );

        let ready = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ready_cb = ready.clone();

        // Cheap: just flips a flag, invoked from within device.poll() on
        // the render thread once the copy is done. No I/O, no encoding.
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                if result.is_ok() {
                    ready_cb.store(true, std::sync::atomic::Ordering::Release);
                }
            });

        // Zero-padded so the filenames sort the same way lexicographically
        // as numerically. Unpadded names ("9.png", "10.png") scramble in any
        // file browser, image viewer or shell glob -- 0, 1, 10, 100, ..., 11,
        // 110 -- which silently reorders a frame sequence. That once made a
        // 13-hour eclipse movie appear to contain fourteen separate eclipses
        // instead of two, and looked convincingly like a rendering bug.
        //
        // Six digits also matches ffmpeg's "%06d" input pattern directly.
        let path = self
            .export_dir
            .join(format!("{:06}.png", self.next_index));
        self.next_index += 1;

        self.in_flight.push(InFlightExport {
            buffer,
            buffer_size,
            ready,
            padded_bytes_per_row,
            unpadded_bytes_per_row,
            width,
            height,
            bgra_swap,
            path,
        });

        if self.sync {
            self.save_last_blocking(device);
        } else {
            self.apply_backpressure(device);
        }
    }

    /// Number of frames exported but not yet on disk: copies still running on
    /// the GPU, plus jobs queued for or being encoded by the worker pool.
    fn outstanding(&self) -> usize {
        self.in_flight.len() + self.pending.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Blocks the render loop while more than `max_queued` frames are
    /// outstanding, so export memory stays bounded.
    ///
    /// Without this the queue is unbounded: whenever the render loop is
    /// faster than the encoders -- which it is by ~100x on a fast GPU -- the
    /// backlog grows until the process dies, and every frame still queued at
    /// that point is lost. Blocking here trades the (fictitious) uncapped
    /// frame rate for one that reflects what actually reaches disk.
    ///
    /// `Wait` rather than `Poll` so a blocked render thread isn't spinning:
    /// `drain_ready` can only make progress once copies actually complete.
    fn apply_backpressure(&mut self, device: &wgpu::Device) {
        if self.max_queued == 0 {
            return;
        }

        while self.outstanding() > self.max_queued {
            // If the copies are all done and the workers are the bottleneck,
            // there is nothing for the device to wait on -- yield instead of
            // burning the core.
            if self.in_flight.is_empty() {
                std::thread::yield_now();
            } else {
                device
                    .poll(wgpu::PollType::Wait {
                        submission_index: None,
                        timeout: None,
                    })
                    .unwrap();
            }

            if !self.drain_ready() {
                return;
            }
        }
    }

    /// Force-completes the copy just queued by `export_frame` and saves it
    /// on the calling (render) thread, so no more than one frame's buffer
    /// is ever alive. This is the whole cost of `Config::export_sync`: a
    /// full GPU stall plus PNG encode and disk write inline, per frame.
    ///
    /// Encoding here rather than handing off to the worker pool is
    /// deliberate -- a handoff plus a wait would pay the same stall and
    /// still need the render thread parked until the workers drained.
    fn save_last_blocking(&mut self, device: &wgpu::Device) {
        while !self
            .in_flight
            .last()
            .is_some_and(|f| f.ready.load(std::sync::atomic::Ordering::Acquire))
        {
            if self.in_flight.is_empty() {
                return;
            }

            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                })
                .unwrap();
        }

        let job = self.in_flight.pop().unwrap();
        save_job(SaveJob {
            buffer: job.buffer,
            buffer_size: job.buffer_size,
            pool_tx: self.pool_tx.clone(),
            padded_bytes_per_row: job.padded_bytes_per_row,
            unpadded_bytes_per_row: job.unpadded_bytes_per_row,
            width: job.width,
            height: job.height,
            bgra_swap: job.bgra_swap,
            path: job.path,
        });
    }
}

impl Drop for FrameExporter {
    /// Safety net for shutdown paths that don't call `finish` explicitly.
    /// Unlike `finish`, this has no `&wgpu::Device` to force-complete any
    /// still-in_flight copies, so those are abandoned -- call `finish`
    /// explicitly (e.g. in App::exit) to guarantee nothing queued is lost.
    fn drop(&mut self) {
        self.save_tx.take();
        for handle in self.save_workers.drain(..) {
            let _ = handle.join();
        }
    }
}
