//! Which facet does each pixel see, read back from the GPU.
//!
//! The main render pass produces a colour image. For a scientific product --
//! a simulated instrument frame whose pixel values *are* the deliverable --
//! colour is the wrong intermediate: it quantises to 8 bits, it is entangled
//! with lighting and tone mapping, and recovering a physical quantity from it
//! means inverting a colormap.
//!
//! This renders the same geometry through the same camera into an `R32Uint`
//! target holding `1 + offset + facet` per pixel, 0 where nothing is drawn.
//! Reading that back gives an exact facet index per pixel, from which any
//! per-facet quantity can be mapped to pixels at full precision in numpy:
//! band radiance in a chosen filter, temperature, emission angle.
//!
//! It also answers visibility for free. The pass carries its own depth
//! buffer, so occlusion -- one body in front of another, a limb hiding the
//! far side -- is resolved by the rasteriser, and a facet absent from the
//! readback is a facet the instrument cannot see.
//!
//! Bodies share one index space through a per-body `offset`, so a single
//! readback identifies both which body and which of its facets.

/// Uniform buffers with a dynamic offset must be aligned to 256 bytes.
const PARAMS_STRIDE: u64 = 256;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct IdParams {
    offset: u32,
    _pad: [u32; 3],
}

pub struct FacetIdPass {
    pipeline: wgpu::RenderPipeline,
    params_layout: wgpu::BindGroupLayout,
    params_buffer: wgpu::Buffer,
    params_bind_group: wgpu::BindGroup,
    params_capacity: usize,

    texture: wgpu::Texture,
    view: wgpu::TextureView,
    depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    width: u32,
    height: u32,
}

impl FacetIdPass {
    pub fn new(
        device: &wgpu::Device,
        camera_layout: &wgpu::BindGroupLayout,
        width: u32,
        height: u32,
    ) -> Self {
        let shader = device.create_shader_module(super::gpu::SHADER_FACET_ID);

        let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("facet_id params"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<IdParams>() as u64
                    ),
                },
                count: None,
            }],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("facet_id"),
            bind_group_layouts: &[Some(camera_layout), Some(&params_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("facet_id"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                // The same three buffers `MeshBuffer::render` binds. The
                // attrib buffer is declared but unused by this shader -- the
                // slot has to line up so the instance data lands in slot 2.
                buffers: &[
                    crate::mesh::Vertex::geometry_desc(),
                    crate::mesh::Vertex::attrib_desc(),
                    super::gpu::MeshBuffer::desc(),
                ],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R32Uint,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                // Unculled deliberately. A shape model with inconsistent
                // winding would otherwise drop facets from the readback, and
                // a missing facet is indistinguishable from an occluded one.
                // Depth still resolves which surface is in front.
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: super::gpu::DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        let (texture, view, depth, depth_view) = Self::targets(device, width, height);
        let (params_buffer, params_bind_group, params_capacity) =
            Self::params(device, &params_layout, 4);

        Self {
            pipeline,
            params_layout,
            params_buffer,
            params_bind_group,
            params_capacity,
            texture,
            view,
            depth,
            depth_view,
            width,
            height,
        }
    }

    fn params(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        n: usize,
    ) -> (wgpu::Buffer, wgpu::BindGroup, usize) {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("facet_id params"),
            size: PARAMS_STRIDE * n.max(1) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("facet_id params"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<IdParams>() as u64),
                }),
            }],
        });
        (buffer, bind_group, n.max(1))
    }

    fn targets(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (
        wgpu::Texture,
        wgpu::TextureView,
        wgpu::Texture,
        wgpu::TextureView,
    ) {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("facet_id"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Uint,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("facet_id depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: super::gpu::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let depth_view = depth.create_view(&Default::default());
        (texture, view, depth, depth_view)
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == self.width && height == self.height {
            return;
        }
        let (texture, view, depth, depth_view) = Self::targets(device, width, height);
        self.texture = texture;
        self.view = view;
        self.depth = depth;
        self.depth_view = depth_view;
        self.width = width;
        self.height = height;
    }

    /// Render the ID map and read it back as `height` rows of `width` indices.
    ///
    /// `meshes` excludes the light cube, and must be in the same order the
    /// caller uses to interpret the returned offsets. Returns the pixel map
    /// alongside the offset applied to each mesh, so the caller can turn a
    /// global index back into `(body, facet)` without recomputing it.
    pub fn render_and_read(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera_bind_group: &wgpu::BindGroup,
        meshes: &[super::gpu::MeshBuffer],
    ) -> (Vec<u32>, Vec<u32>) {
        if meshes.len() > self.params_capacity {
            let (b, g, c) = Self::params(device, &self.params_layout, meshes.len());
            self.params_buffer = b;
            self.params_bind_group = g;
            self.params_capacity = c;
        }

        // Offsets stack so every facet of every body has a distinct index.
        let mut offsets = Vec::with_capacity(meshes.len());
        let mut acc = 0u32;
        for mesh in meshes {
            offsets.push(acc);
            queue.write_buffer(
                &self.params_buffer,
                PARAMS_STRIDE * offsets.len() as u64 - PARAMS_STRIDE,
                bytemuck::bytes_of(&IdParams {
                    offset: acc,
                    _pad: [0; 3],
                }),
            );
            acc += mesh.n_facets();
        }

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("facet_id"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // 0 is "no facet": the background must not decode to
                        // facet 0 of body 0.
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, camera_bind_group, &[]);

            for (i, mesh) in meshes.iter().enumerate() {
                if !mesh.is_flat {
                    // `vertex_index / 3` is only the facet index when each
                    // facet owns its three vertices. Indexed meshes share
                    // them, so the mapping would be wrong -- skip rather than
                    // return indices that look valid and are not.
                    continue;
                }
                pass.set_bind_group(
                    1,
                    &self.params_bind_group,
                    &[(PARAMS_STRIDE * i as u64) as u32],
                );
                mesh.render(&mut pass);
            }
        }

        // Readback rows are padded to 256 bytes; unpad on the CPU.
        let bytes_per_row = self.width * 4;
        let padded = bytes_per_row.div_ceil(256) * 256;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("facet_id readback"),
            size: (padded * self.height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));

        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .unwrap();

        let data = slice.get_mapped_range();
        let mut out = Vec::with_capacity((self.width * self.height) as usize);
        for row in 0..self.height {
            let start = (row * padded) as usize;
            let end = start + bytes_per_row as usize;
            out.extend_from_slice(bytemuck::cast_slice::<u8, u32>(&data[start..end]));
        }
        drop(data);
        staging.unmap();

        (out, offsets)
    }
}

// Silence the unused-field warning: kept so the textures outlive their views.
impl FacetIdPass {
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}
