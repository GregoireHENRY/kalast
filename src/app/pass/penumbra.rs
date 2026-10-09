//! The Sun's disc, walked in passes of its own (`light.sun_as_point` off).
//!
//! What a receiver sees of the disc was found where it is shaded, in the
//! main pass's fragment stage: the depth pyramid looked round every lit
//! pixel for the few in a penumbra, and each of those walked the shadow map
//! along 32 directions, one after another. A fragment stage gives a pixel
//! one lane of 32, so a walk kept the lanes beside it idle and waited on
//! each of its reads in turn: on Didymos from 1 km with Dimorphos's shadow
//! across it, 25 ms of a 31 ms frame. Now, before the main pass:
//!
//! 1. **A prepass** draws the bodies once more, single-sampled, writing each
//!    pixel's surface -- its normal and shadow layer -- and its depth
//!    (`fs_penumbra`), and nothing else: a fragment stage that writes to
//!    memory is run for hidden fragments too, and one that queued its own
//!    walks queued 270,000 for the 50,000 pixels of that frame that needed
//!    one.
//! 2. **The scan**, a compute pass over the pixels (`cs_scan`): each one's
//!    place from its depth through the camera's inverse, then the pyramid's
//!    look round (`sun_reach`) -- no penumbra, the umbra, or a walk, queued
//!    a workgroup at a time -- into a code per pixel.
//! 3. **The walks**, a compute pass over the queue (`cs_walk`): an
//!    invocation per receiver and direction, so a receiver's 32 directions
//!    run side by side and as many receivers are in flight as the GPU holds.
//! 4. **The main pass** reads its pixel's code where the prepass saw the
//!    same surface there -- its depth, to twice its change over a pixel --
//!    or a neighbour's that did, and takes the hard lookup where none did.
//!
//! The textures and the queue are made the first time the disc is on, at
//! the image's size; until then the main pass binds `idle`.

use crate::app::gpu;
use wgpu::util::DeviceExt;

/// Workgroups of 64 the walks are dispatched as, each invocation taking
/// every `64 * WALK_GROUPS`-th of the queue's receiver-directions: enough to
/// fill the GPU however long the queue is.
const WALK_GROUPS: u32 = 1024;

/// One queued walk, `Walk` in `mesh_shadow.wgsl`.
const WALK_BYTES: u64 = 48;

/// Room for walks in the per-facet query with the Sun a disc: four points a
/// facet in a penumbra, 50,000 of them on Didymos with Dimorphos's shadow
/// across it. A point past the room is walked where it was found.
const FACET_WALKS: u64 = 1 << 18;

pub struct Pass {
    full: wgpu::RenderPipeline,
    /// For a flat mesh drawn indexed, as the main pass's own lean build;
    /// `None` without `primitive_index`.
    lean: Option<wgpu::RenderPipeline>,
    scan: wgpu::ComputePipeline,
    walk: wgpu::ComputePipeline,
    /// The per-facet query with the Sun a disc: its points (`cs_facets`),
    /// their walks (`cs_walk` again) and the walked points' shares
    /// (`cs_facets_walked`); its group 6 -- a queue as the image's, what each
    /// walk is for, the query, the body's vertices and corners, the answer;
    /// and the queue, made the first time.
    facets: [wgpu::ComputePipeline; 3],
    facets_layout: wgpu::BindGroupLayout,
    facet_queue: std::cell::OnceCell<(wgpu::Buffer, wgpu::Buffer)>,
    primitive_index: bool,
    immediates: bool,
    /// Group 6 for the scan and the walks: the queue, the prepass's
    /// surfaces and depth, the codes, the camera's inverse.
    write_layout: wgpu::BindGroupLayout,
    /// Group 6 as the main pass reads it: the queue and the codes.
    pub read_layout: wgpu::BindGroupLayout,
    targets: Option<Targets>,
    /// The main pass's group 6 while there are no targets.
    idle: wgpu::BindGroup,
}

struct Targets {
    size: (u32, u32),
    _surface: wgpu::Texture,
    surface_view: wgpu::TextureView,
    _depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    _codes: wgpu::Texture,
    queue: wgpu::Buffer,
    /// `Scan` in `mesh_shadow.wgsl`.
    scan: wgpu::Buffer,
    write: wgpu::BindGroup,
    read: wgpu::BindGroup,
}

/// Whether the disc is walked this frame: on, and the image shaded with
/// shadows -- the unlit modes and `color_mode = 3` read no shadow.
pub fn wanted(config: &crate::app::config::Config) -> bool {
    config.light.disc_radius() > 0.0 && !matches!(config.shading.color_mode, 1..=3)
}

impl Pass {
    /// `shaded` is the main pass's layouts up to group 5, of which the scan
    /// and the walks bind the first three too.
    pub fn new(
        device: &wgpu::Device,
        config: &crate::app::config::Config,
        shaded: &[Option<&wgpu::BindGroupLayout>],
    ) -> Self {
        let entry = |binding, visibility, ty| wgpu::BindGroupLayoutEntry { binding, visibility, ty, count: None };
        let storage = |read_only| wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let texture = |sample_type| wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let compute = wgpu::ShaderStages::COMPUTE;
        let write_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("penumbra scan"),
            entries: &[
                entry(0, compute, storage(false)),
                entry(3, compute, texture(wgpu::TextureSampleType::Uint)),
                entry(4, compute, texture(wgpu::TextureSampleType::Depth)),
                entry(
                    5,
                    compute,
                    wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rg32Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                ),
                entry(
                    6,
                    compute,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
            ],
        });
        let read_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("penumbra walked"),
            entries: &[
                entry(1, wgpu::ShaderStages::FRAGMENT, storage(true)),
                entry(2, wgpu::ShaderStages::FRAGMENT, texture(wgpu::TextureSampleType::Uint)),
            ],
        });

        // Culled and drawn as the main pass draws (`render::Pass::new`), so
        // the prepass sees what it will.
        let mirrored = config.image.flip_x != config.image.flip_y;
        let cull_mode = if config.shading.render_back_face {
            None
        } else if mirrored {
            Some(wgpu::Face::Front)
        } else {
            Some(wgpu::Face::Back)
        };
        let primitive_index = gpu::has_primitive_index(device);
        let immediates = gpu::has_immediates(device) && primitive_index;
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("penumbra prepass"),
            bind_group_layouts: shaded,
            immediate_size: if immediates { 8 } else { 0 },
        });
        let prepass = |lean| {
            let module = device.create_shader_module(gpu::shader_for(device, &gpu::SHADER_MESH_SHADOW, lean));
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("penumbra prepass"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    buffers: &[
                        Some(crate::mesh::Vertex::geometry_desc()),
                        Some(gpu::MeshBuffer::desc()),
                    ],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_penumbra"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba32Uint,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: gpu::DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(gpu::DEPTH_COMPARE),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let full = prepass(false);
        let lean = primitive_index.then(|| prepass(true));

        // The globals, the view and the shadow layers, as the main pass has
        // them, and the scan's own.
        let module = device.create_shader_module(gpu::shader_for(device, &gpu::SHADER_MESH_SHADOW, false));
        let compute_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("penumbra scan and walks"),
            bind_group_layouts: &[shaded[0], shaded[1], shaded[2], None, None, None, Some(&write_layout)],
            immediate_size: 0,
        });
        let compute = |entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&compute_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let (scan, walk) = (compute("cs_scan"), compute("cs_walk"));

        // The globals, the view, the shadow layers and the body's own group,
        // as the main pass has them, and the query's.
        let facets_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("facet shadows, disc"),
            entries: &[
                entry(
                    10,
                    wgpu::ShaderStages::COMPUTE,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(11, wgpu::ShaderStages::COMPUTE, storage(true)),
                entry(12, wgpu::ShaderStages::COMPUTE, storage(true)),
                entry(13, wgpu::ShaderStages::COMPUTE, storage(false)),
                entry(0, wgpu::ShaderStages::COMPUTE, storage(false)),
                entry(14, wgpu::ShaderStages::COMPUTE, storage(false)),
            ],
        });
        let facets_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("facet shadows, disc"),
            bind_group_layouts: &[shaded[0], shaded[1], shaded[2], None, None, shaded[5], Some(&facets_layout)],
            immediate_size: 0,
        });
        let facets = ["cs_facets", "cs_walk", "cs_facets_walked"].map(|entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&facets_pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        });

        let idle = Self::targets_of(device, &write_layout, &read_layout, (1, 1)).read;
        Self {
            full,
            lean,
            scan,
            walk,
            facets,
            facets_layout,
            facet_queue: std::cell::OnceCell::new(),
            primitive_index,
            immediates,
            write_layout,
            read_layout,
            targets: None,
            idle,
        }
    }

    /// What each facet of `mesh` hides of the Sun, in `[0, 1]`, 0 wholly
    /// lit: the thermophysical model's shadows, as the image has them -- the
    /// same layer, slices, lookups and PCF kernel, and with the Sun a disc the
    /// same pyramid and walk -- averaged over the facet's corners and centre.
    /// After a frame was drawn, from its maps; blocking, the answer read back
    /// before it returns.
    pub fn facets(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bindings: &super::Bindings,
        mesh: &gpu::MeshBuffer,
    ) -> Vec<f32> {
        let n = mesh.n_facets();
        if n == 0 {
            return vec![];
        }
        let (groups_x, groups_y, stride) = crate::gpu::Context::dispatch_2d(n as u64);
        let (queue_buffer, walks_for) = self.facet_queue.get_or_init(|| {
            let buffer = |label, size| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            };
            (
                buffer("facet shadows, disc, walks", 8 + FACET_WALKS * WALK_BYTES),
                buffer("facet shadows, disc, what each walk is for", FACET_WALKS * 16),
            )
        });
        let query = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("facet shadows, disc"),
            contents: bytemuck::cast_slice(&[n, stride, mesh.is_flat as u32, 0u32]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let size = n as u64 * 4;
        let out = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("facet shadows, disc"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("facet shadows, disc, read"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("facet shadows, disc"),
            layout: &self.facets_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: queue_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 10, resource: query.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 11, resource: mesh.geometry_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 12, resource: mesh.index_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 13, resource: out.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 14, resource: walks_for.as_entire_binding() },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.clear_buffer(queue_buffer, 0, Some(8));
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("facet shadows, disc"),
                timestamp_writes: None,
            });
            pass.set_bind_group(0, Some(&bindings.globals), &[]);
            pass.set_bind_group(1, Some(&bindings.view), &[]);
            pass.set_bind_group(2, Some(&bindings.shadow), &[]);
            pass.set_bind_group(5, Some(&mesh.attr_bind_group), &[]);
            pass.set_bind_group(6, Some(&group), &[]);
            pass.set_pipeline(&self.facets[0]);
            pass.dispatch_workgroups(groups_x, groups_y, 1);
            pass.set_pipeline(&self.facets[1]);
            pass.dispatch_workgroups(WALK_GROUPS, 1, 1);
            pass.set_pipeline(&self.facets[2]);
            pass.dispatch_workgroups(256, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&out, 0, &read, 0, size);
        queue.submit(Some(encoder.finish()));
        let slice = read.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device
            .poll(wgpu::PollType::Wait { submission_index: None, timeout: None })
            .unwrap();
        let data = slice.get_mapped_range().unwrap();
        // `FACET_FIXED` in `mesh_shadow.wgsl` a whole disc's worth per point,
        // four points a facet.
        let answer: Vec<f32> =
            bytemuck::cast_slice::<u8, u32>(&data).iter().map(|&s| 1.0 - s as f32 / (4.0 * 4_194_304.0)).collect();
        drop(data);
        read.unmap();
        answer
    }

    fn targets_of(
        device: &wgpu::Device,
        write_layout: &wgpu::BindGroupLayout,
        read_layout: &wgpu::BindGroupLayout,
        size: (u32, u32),
    ) -> Targets {
        let extent = wgpu::Extent3d { width: size.0.max(1), height: size.1.max(1), depth_or_array_layers: 1 };
        let texture = |format, usage, label| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: usage | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let drawn = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let surface = texture(wgpu::TextureFormat::Rgba32Uint, drawn, "penumbra surfaces");
        let depth = texture(gpu::DEPTH_FORMAT, drawn, "penumbra depth");
        let codes = texture(wgpu::TextureFormat::Rg32Uint, wgpu::TextureUsages::STORAGE_BINDING, "penumbra codes");
        let surface_view = surface.create_view(&Default::default());
        let depth_view = depth.create_view(&Default::default());
        let codes_view = codes.create_view(&Default::default());
        let scan = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("penumbra scan"),
            size: 144,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Room for a walk per eight pixels -- on Didymos from 1 km, one
        // pixel in fifteen walks; a pixel past the room finds its disc in
        // the main pass.
        let walks = (extent.width as u64 * extent.height as u64 / 8).max(65_536);
        let queue = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("penumbra queue"),
            size: 8 + walks * WALK_BYTES,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view = wgpu::BindingResource::TextureView;
        let write = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("penumbra scan"),
            layout: write_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: queue.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: view(&surface_view) },
                wgpu::BindGroupEntry { binding: 4, resource: view(&depth_view) },
                wgpu::BindGroupEntry { binding: 5, resource: view(&codes_view) },
                wgpu::BindGroupEntry { binding: 6, resource: scan.as_entire_binding() },
            ],
        });
        let read = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("penumbra walked"),
            layout: read_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 1, resource: queue.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: view(&codes_view) },
            ],
        });
        Targets {
            size,
            _surface: surface,
            surface_view,
            _depth: depth,
            depth_view,
            _codes: codes,
            queue,
            scan,
            write,
            read,
        }
    }

    /// The main pass's group 6: what this frame's passes found, or `idle`.
    pub fn read_group(&self) -> &wgpu::BindGroup {
        self.targets.as_ref().map_or(&self.idle, |t| &t.read)
    }

    /// The prepass, the scan and the walks, for an image `size` pixels seen
    /// through `view_proj`: before the main pass, and only when `wanted`,
    /// which the light's `penumbra` flag says to the main pass.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        meshes: &[gpu::MeshBuffer],
        bindings: &super::Bindings,
        config: &crate::app::config::Config,
        size: (u32, u32),
        view_proj: crate::Mat4,
        timer: Option<&crate::app::gpu_timing::GpuTimer>,
    ) {
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            self.targets = Some(Self::targets_of(device, &self.write_layout, &self.read_layout, size));
        }
        let t = self.targets.as_ref().unwrap();
        let mut scan = [0u32; 36];
        scan[..16].copy_from_slice(bytemuck::cast_slice(&gpu::to_cols_f32(view_proj.inverse())));
        scan[16..32].copy_from_slice(bytemuck::cast_slice(&gpu::to_cols_f32(view_proj)));
        scan[32..34].copy_from_slice(&[size.0, size.1]);
        queue.write_buffer(&t.scan, 0, bytemuck::cast_slice(&scan));
        encoder.clear_buffer(&t.queue, 0, Some(8));
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("penumbra prepass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.surface_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(gpu::DEPTH_CLEAR),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: timer.and_then(|t| t.scope(crate::app::gpu_timing::Scope::Penumbra)),
                ..Default::default()
            });
            pass.set_pipeline(&self.full);
            bindings.all(&mut pass);
            // Drawn as the main pass draws them (`render::Pass::render`).
            let corners = config.wireframe.mode != 0 || !self.primitive_index;
            let mut lean_set = false;
            // Not a body whose layer is empty: there is no penumbra there to
            // find, and with a horizon map, Mars's 12.9M facets are most of
            // what this pass would draw.
            for mesh in meshes[1..].iter().filter(|m| !m.unshadowed) {
                let lean = self.lean.as_ref().filter(|_| mesh.is_flat && !corners);
                if lean.is_some() != lean_set {
                    pass.set_pipeline(lean.unwrap_or(&self.full));
                    lean_set = lean.is_some();
                }
                mesh.render_shaded(&mut pass, corners, self.immediates);
            }
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("penumbra scan and walks"),
            timestamp_writes: timer.and_then(|t| t.compute_scope(crate::app::gpu_timing::Scope::Penumbra)),
        });
        pass.set_bind_group(0, Some(&bindings.globals), &[]);
        pass.set_bind_group(1, Some(&bindings.view), &[]);
        pass.set_bind_group(2, Some(&bindings.shadow), &[]);
        pass.set_bind_group(6, Some(&t.write), &[]);
        pass.set_pipeline(&self.scan);
        pass.dispatch_workgroups(size.0.div_ceil(8), size.1.div_ceil(8), 1);
        pass.set_pipeline(&self.walk);
        pass.dispatch_workgroups(WALK_GROUPS, 1, 1);
    }
}
