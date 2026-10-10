//! Shadows from the Sun traced with rays: the GPU's ray queries (wgpu's
//! `EXPERIMENTAL_RAY_QUERY`) against one bottom-level acceleration structure
//! per body, built once from its full-resolution mesh -- never a level of
//! detail's cut, which would be a physics error in the thermophysical model
//! -- and a top level placing the bodies, rebuilt every frame from their
//! matrices. Coordinates stay body-local in the bottom levels.
//!
//! On by `shadows.rays`, where the device has ray queries: Vulkan on Windows
//! and Linux (DX12 has them only with DXC, which kalast does not ship), Metal
//! on macOS 15 and later. Elsewhere the shadow maps answer as before. See
//! `notes/2026-10-10_ray_traced_shadows/`.

use wgpu::util::DeviceExt;

/// Whether the adapter can trace rays from a shader.
pub fn supported(adapter: &wgpu::Adapter) -> bool {
    adapter
        .features()
        .contains(wgpu::Features::from(wgpu::FeaturesWGPU::EXPERIMENTAL_RAY_QUERY))
}

/// The uniform of `cs_facets` in `raytrace.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FacetRays {
    mat: [[f32; 4]; 4],
    sun: [f32; 3],
    sun_radius: f32,
    n_facets: u32,
    stride: u32,
    is_flat: u32,
    samples: u32,
    probe: u32,
    _pad: [u32; 3],
}

pub struct RayTracing {
    /// One per body, `None` for a body with no surface.
    blas: Vec<Option<wgpu::Blas>>,
    /// The window's `mesh_epoch` the bottom levels were built for.
    epoch: Option<u64>,
    tlas: wgpu::Tlas,
    /// Each body's matrix as the top level has it, for the per-facet query
    /// to place the body's corners where its rays see them.
    mats: Vec<crate::Mat4>,
    facets_layout: wgpu::BindGroupLayout,
    facets_pipeline: wgpu::ComputePipeline,
    /// The top level for the main pass, at group 7 (`mesh_shadow.wgsl`):
    /// bound every frame on a device that traces, made again with the top
    /// level.
    pub image_layout: wgpu::BindGroupLayout,
    pub image_group: wgpu::BindGroup,
    /// The bodies' surfaces while light bounced off them is wanted, and a
    /// placeholder for the bindings otherwise.
    geometry: Option<Geometry>,
    empty: Geometry,
}

/// Every body's surface as the rays meet it, for light bounced off it
/// (`ray_bounced` in `sun_rays_image.wgsl`): the vertices of all of them in
/// one buffer, the facets' indices into it, where each body's facets begin,
/// and each body's reflectance -- the mean of its facets' colours, a Lambert
/// surface's albedo. A ray's hit names its body (the instance's custom
/// data) and its facet, which find the facet's corners, its normal in the
/// body's frame, and its body's albedo.
struct Geometry {
    positions: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_base: wgpu::Buffer,
    albedo: wgpu::Buffer,
}

impl Geometry {
    fn of(device: &wgpu::Device, simulation: &crate::app::simulation::Simulation) -> Self {
        let mut positions: Vec<f32> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut index_base: Vec<u32> = Vec::new();
        let mut albedo: Vec<[f32; 4]> = Vec::new();
        for body in &simulation.bodies {
            index_base.push(indices.len() as u32);
            let Some(mesh) = body.mesh.as_ref() else {
                albedo.push([0.0; 4]);
                continue;
            };
            let mesh = mesh.borrow();
            let base = (positions.len() / 3) as u32;
            positions.extend(mesh.positions.iter().flat_map(|p| [p.x as f32, p.y as f32, p.z as f32]));
            let whole = mesh.indices.len() / 3 * 3;
            indices.extend(mesh.indices[..whole].iter().map(|&i| base + i));
            let n = mesh.attrs.len().max(1) as f32;
            let sum = mesh.attrs.iter().fold([0.0f32; 3], |s, a| {
                [s[0] + a.color.x as f32, s[1] + a.color.y as f32, s[2] + a.color.z as f32]
            });
            albedo.push([sum[0] / n, sum[1] / n, sum[2] / n, 1.0]);
        }
        let buffer = |label, contents: &[u8]| {
            let contents = if contents.is_empty() { &[0u8; 16][..] } else { contents };
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some(label), contents, usage: wgpu::BufferUsages::STORAGE })
        };
        Self {
            positions: buffer("the bodies' vertices, for bounced light", bytemuck::cast_slice(&positions)),
            indices: buffer("the bodies' facets, for bounced light", bytemuck::cast_slice(&indices)),
            index_base: buffer("where each body's facets begin", bytemuck::cast_slice(&index_base)),
            albedo: buffer("each body's albedo", bytemuck::cast_slice(&albedo)),
        }
    }

    /// Bindings with nothing in them, while no light is bounced.
    fn empty(device: &wgpu::Device) -> Self {
        let buffer = |label| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some(label), contents: &[0u8; 16], usage: wgpu::BufferUsages::STORAGE })
        };
        Self {
            positions: buffer("no vertices"),
            indices: buffer("no facets"),
            index_base: buffer("no bodies"),
            albedo: buffer("no albedo"),
        }
    }
}

impl RayTracing {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let module = device.create_shader_module(crate::app::gpu::shader_for(device, &crate::app::gpu::SHADER_RAYTRACE, false));
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let facets_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ray-traced facet shadows"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::AccelerationStructure { vertex_return: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                storage(2, true),
                storage(3, true),
                storage(4, false),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ray-traced facet shadows"),
            bind_group_layouts: &[Some(&facets_layout)],
            immediate_size: 0,
        });
        let facets_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("ray-traced facet shadows"),
            layout: Some(&layout),
            module: &module,
            entry_point: Some("cs_facets"),
            compilation_options: Default::default(),
            cache: None,
        });
        let fragment_storage = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let image_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ray-traced shadows, the image"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::AccelerationStructure { vertex_return: false },
                    count: None,
                },
                // The bodies' surfaces, for light bounced off them
                // (`Geometry`, `sun_rays_image.wgsl`).
                fragment_storage(1),
                fragment_storage(2),
                fragment_storage(3),
                fragment_storage(4),
            ],
        });
        let tlas = Self::tlas_for(device, 8);
        // Built empty at once: the main pass binds it every frame, rays or
        // not, and a top level never built may not be bound.
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.build_acceleration_structures(std::iter::empty(), std::iter::once(&tlas));
        queue.submit(Some(encoder.finish()));
        let empty = Geometry::empty(device);
        let image_group = Self::image_group_for(device, &image_layout, &tlas, &empty);
        Self {
            blas: Vec::new(),
            epoch: None,
            tlas,
            mats: Vec::new(),
            facets_layout,
            facets_pipeline,
            image_layout,
            image_group,
            geometry: None,
            empty,
        }
    }

    fn image_group_for(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, tlas: &wgpu::Tlas, geometry: &Geometry) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ray-traced shadows, the image"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: tlas.as_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: geometry.positions.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: geometry.indices.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: geometry.index_base.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: geometry.albedo.as_entire_binding() },
            ],
        })
    }

    fn rebind(&mut self, device: &wgpu::Device) {
        let geometry = self.geometry.as_ref().unwrap_or(&self.empty);
        self.image_group = Self::image_group_for(device, &self.image_layout, &self.tlas, geometry);
    }

    fn tlas_for(device: &wgpu::Device, max_instances: u32) -> wgpu::Tlas {
        device.create_tlas(&wgpu::CreateTlasDescriptor {
            label: Some("the bodies"),
            max_instances,
            flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE,
            update_mode: wgpu::AccelerationStructureUpdateMode::Build,
        })
    }

    /// The acceleration structures for this frame: each body's bottom level
    /// built again when the window's meshes were (`epoch`), the top level
    /// placing the bodies where they are now.
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        simulation: &crate::app::simulation::Simulation,
        epoch: u64,
        // Light bounced off the surfaces is wanted (`reference.bounces`):
        // their geometry kept, for the hits' normals and reflectance.
        bounces: bool,
    ) {
        let n = simulation.bodies.len();
        let mut rebind = false;
        if self.epoch != Some(epoch) || self.blas.len() != n {
            self.build_bodies(device, queue, simulation);
            self.epoch = Some(epoch);
            self.geometry = None;
            rebind = true;
        }
        if bounces && self.geometry.is_none() {
            self.geometry = Some(Geometry::of(device, simulation));
            rebind = true;
        } else if !bounces && self.geometry.is_some() {
            self.geometry = None;
            rebind = true;
        }
        if self.tlas.get().len() < n {
            self.tlas = Self::tlas_for(device, (n as u32).next_power_of_two());
            rebind = true;
        }
        if rebind {
            self.rebind(device);
        }
        self.mats = simulation.bodies.iter().map(|b| b.mat).collect();
        let slots = self.tlas.get().len();
        for i in 0..slots {
            let instance = self.blas.get(i).and_then(|b| b.as_ref()).map(|blas| {
                wgpu::TlasInstance::new(blas, rows_3x4(&self.mats[i]), i as u32, 0xff)
            });
            if let Some(slot) = self.tlas.get_mut_single(i) {
                *slot = instance;
            }
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("the bodies' top level"),
        });
        encoder.build_acceleration_structures(std::iter::empty(), std::iter::once(&self.tlas));
        queue.submit(Some(encoder.finish()));
    }

    /// Every body's bottom level, from its mesh as loaded: the shared
    /// vertices and the facets' indices, in the body's own frame. In a
    /// submission of their own, before any top level is built over them --
    /// Metal did not order the two within one (gfx-rs/wgpu #9215).
    fn build_bodies(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, simulation: &crate::app::simulation::Simulation) {
        self.blas.clear();
        let mut inputs = Vec::new();
        for body in &simulation.bodies {
            let Some(mesh) = body.mesh.as_ref() else {
                self.blas.push(None);
                continue;
            };
            let mesh = mesh.borrow();
            if mesh.indices.len() < 3 || mesh.positions.is_empty() {
                self.blas.push(None);
                continue;
            }
            let positions: Vec<[f32; 3]> = mesh
                .positions
                .iter()
                .map(|p| [p.x as f32, p.y as f32, p.z as f32])
                .collect();
            let n_indices = (mesh.indices.len() / 3 * 3) as u32;
            let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("a body's vertices, for its acceleration structure"),
                contents: bytemuck::cast_slice(&positions),
                usage: wgpu::BufferUsages::BLAS_INPUT,
            });
            let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("a body's facets, for its acceleration structure"),
                contents: bytemuck::cast_slice(&mesh.indices[..n_indices as usize]),
                usage: wgpu::BufferUsages::BLAS_INPUT,
            });
            let size = wgpu::BlasTriangleGeometrySizeDescriptor {
                vertex_format: wgpu::VertexFormat::Float32x3,
                vertex_count: positions.len() as u32,
                index_format: Some(wgpu::IndexFormat::Uint32),
                index_count: Some(n_indices),
                flags: wgpu::AccelerationStructureGeometryFlags::OPAQUE,
            };
            let blas = device.create_blas(
                &wgpu::CreateBlasDescriptor {
                    label: Some("a body"),
                    flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE
                        | wgpu::AccelerationStructureFlags::ALLOW_COMPACTION,
                    update_mode: wgpu::AccelerationStructureUpdateMode::Build,
                },
                wgpu::BlasGeometrySizeDescriptors::Triangles { descriptors: vec![size.clone()] },
            );
            inputs.push((self.blas.len(), size, vertices, indices));
            self.blas.push(Some(blas));
        }
        if inputs.is_empty() {
            return;
        }
        let entries: Vec<wgpu::BlasBuildEntry> = inputs
            .iter()
            .map(|(i, size, vertices, indices)| wgpu::BlasBuildEntry {
                blas: self.blas[*i].as_ref().unwrap(),
                geometry: wgpu::BlasGeometries::TriangleGeometries(vec![wgpu::BlasTriangleGeometry {
                    size,
                    vertex_buffer: vertices,
                    first_vertex: 0,
                    vertex_stride: 12,
                    index_buffer: Some(indices),
                    first_index: Some(0),
                    transform_buffer: None,
                    transform_buffer_offset: None,
                }]),
            })
            .collect();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("the bodies' bottom levels"),
        });
        encoder.build_acceleration_structures(entries.iter(), std::iter::empty());
        queue.submit(Some(encoder.finish()));
        drop(entries);

        // Compacted, once built: a build reserves for the worst case, and
        // the copy holds what the bodies took. Turning the rays on cost the
        // Didymos pair, two of 3.1 million triangles, 770 MiB of the GPU's
        // memory, and costs 258 compacted -- which the Mac, sharing its
        // memory with everything, needs most.
        for blas in self.blas.iter().flatten() {
            blas.prepare_compaction_async(|_| {});
        }
        let _ = device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
        for slot in self.blas.iter_mut() {
            if let Some(blas) = slot.as_ref().filter(|b| b.ready_for_compaction()) {
                *slot = Some(queue.compact_blas(blas));
            }
        }
        // And waited for, so the copies built first, and their inputs, are
        // let go now: without this they stayed, and the rays cost as much
        // as before compacting.
        queue.submit(None);
        let _ = device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
    }

    /// Per facet of `body`, the share of the Sun's light that does not reach
    /// it, as the shadow maps' query answers (`penumbra::Pass::facets`):
    /// each facet's corners and centre, each tracing `samples` rays across
    /// the Sun's disc of `sun_radius`, one for a point -- `probe` of them
    /// first, the rest only where those disagree. Blocking.
    pub fn facet_shadows(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        body: usize,
        mesh: &crate::app::gpu::MeshBuffer,
        sun: [f32; 3],
        sun_radius: f32,
        samples: u32,
        probe: u32,
    ) -> Vec<f32> {
        let n = mesh.n_facets();
        let Some(mat) = self.mats.get(body) else {
            return vec![];
        };
        if n == 0 {
            return vec![];
        }
        let (groups_x, groups_y, stride) = crate::gpu::Context::dispatch_2d(n as u64);
        let m = mat.to_cols_array_2d();
        let uniform = FacetRays {
            mat: m.map(|c| c.map(|v| v as f32)),
            sun,
            sun_radius,
            n_facets: n,
            stride,
            is_flat: mesh.is_flat as u32,
            samples: samples.max(1),
            probe,
            _pad: [0; 3],
        };
        let query = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ray-traced facet shadows"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let size = n as u64 * 4;
        let out = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ray-traced facet shadows"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ray-traced facet shadows, read"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ray-traced facet shadows"),
            layout: &self.facets_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.tlas.as_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: query.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: mesh.geometry_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: mesh.index_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: out.as_entire_binding() },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ray-traced facet shadows"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.facets_pipeline);
            pass.set_bind_group(0, Some(&group), &[]);
            pass.dispatch_workgroups(groups_x, groups_y, 1);
        }
        encoder.copy_buffer_to_buffer(&out, 0, &read, 0, size);
        queue.submit(Some(encoder.finish()));
        let slice = read.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device
            .poll(wgpu::PollType::Wait { submission_index: None, timeout: None })
            .unwrap();
        let data = slice.get_mapped_range().unwrap();
        let answer = bytemuck::cast_slice::<u8, f32>(&data).to_vec();
        drop(data);
        read.unmap();
        answer
    }
}

/// A body's matrix as a top-level instance takes it: three rows of four.
fn rows_3x4(m: &crate::Mat4) -> [f32; 12] {
    let c = m.to_cols_array_2d();
    let mut r = [0.0f32; 12];
    for row in 0..3 {
        for col in 0..4 {
            r[row * 4 + col] = c[col][row] as f32;
        }
    }
    r
}

#[cfg(test)]
mod tests {
    /// Rows of the matrix, the translation in each row's last place.
    #[test]
    fn a_matrix_goes_in_by_rows() {
        let m = crate::Mat4::from_cols_array_2d(&[
            [1.0, 2.0, 3.0, 0.0],
            [4.0, 5.0, 6.0, 0.0],
            [7.0, 8.0, 9.0, 0.0],
            [10.0, 11.0, 12.0, 1.0],
        ]);
        assert_eq!(super::rows_3x4(&m), [1.0, 4.0, 7.0, 10.0, 2.0, 5.0, 8.0, 11.0, 3.0, 6.0, 9.0, 12.0]);
    }
}
