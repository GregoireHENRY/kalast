//! View factors by hemicube, computed on the GPU.
//!
//! The classic radiosity construction: put the top half of a cube at a facet,
//! render the scene into its five faces with every facet writing its own
//! index, and weight each pixel by its delta form factor. What that buys over
//! pairwise integration is three things at once.
//!
//! - **Occlusion is free and exact.** The depth test already resolves which
//!   surface a pixel sees, so there is no separate visibility pass. The
//!   pairwise form has none at all: it happily radiates through solid rock.
//! - **O(N) render passes, not O(N^2) pair tests.** One hemicube yields the
//!   entire row `VF[i, :]`.
//! - **The near field stops being a special case.** No `1/d^2` to diverge and
//!   no threshold to choose: a close facet simply covers many pixels, and the
//!   sum is bounded at 1 by construction.
//!
//! Validated in `examples/analytical/hemicube.py` against perpendicular unit
//! squares -- 0.07% against the closed form, where the subdivided pairwise
//! reference manages 3.7%.
//!
//! Why the accumulation is on the GPU
//! ----------------------------------
//!
//! The Python prototype read each face back and weighted it in numpy: 18.7 ms
//! per facet, essentially all of it PCIe latency, five blocking round trips
//! per hemicube. Here the atlas never leaves the device -- a compute pass
//! scatters the weights into a per-facet accumulator, and only the finished
//! row comes back, once per batch.

use wgpu::util::DeviceExt;

/// Faces per hemicube: the top, then four sides.
pub const FACES: u32 = 5;

/// Matches `FIXED_SCALE` in the shaders. See there for why fixed point.
const FIXED_SCALE: f32 = 1_073_741_824.0;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct RenderParams {
    view_proj: [[f32; 4]; 4],
    facet_offset: u32,
    _pad: [u32; 3],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct AccumParams {
    row_offset: u32,
    n_facets: u32,
    width: u32,
    height: u32,
}

/// Uniform buffers with a dynamic offset must be 256-byte aligned.
const STRIDE: u64 = 256;

pub struct Hemicube {
    resolution: u32,
    batch: u32,

    render_pipeline: wgpu::RenderPipeline,
    render_layout: wgpu::BindGroupLayout,
    render_params: wgpu::Buffer,
    render_bind: wgpu::BindGroup,

    accum_pipeline: wgpu::ComputePipeline,
    accum_layout: wgpu::BindGroupLayout,
    accum_params: wgpu::Buffer,

    ids: wgpu::Texture,
    ids_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    weights_view: wgpu::TextureView,
}

impl Hemicube {
    /// `resolution` is one face; the atlas is `resolution` by `5 resolution`.
    /// `batch` hemicubes are accumulated before a readback.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, resolution: u32, batch: u32) -> Self {
        let atlas_w = resolution * FACES;
        let atlas_h = resolution;

        // -- render half --------------------------------------------------
        let shader = device.create_shader_module(super::gpu::SHADER_HEMICUBE);
        let render_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hemicube render"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<RenderParams>() as u64
                    ),
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hemicube"),
            bind_group_layouts: &[Some(&render_layout)],
            immediate_size: 0,
        });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hemicube"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
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
                // Unculled: a shape model with inconsistent winding would
                // otherwise drop facets, and a missing facet is
                // indistinguishable from an occluded one. Depth still decides
                // what is in front.
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

        // One slot per face of every hemicube in a batch.
        let render_params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hemicube render params"),
            size: STRIDE * (batch * FACES) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let render_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hemicube render params"),
            layout: &render_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &render_params,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<RenderParams>() as u64),
                }),
            }],
        });

        // -- targets -------------------------------------------------------
        let size = wgpu::Extent3d {
            width: atlas_w,
            height: atlas_h,
            depth_or_array_layers: 1,
        };
        let ids = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hemicube ids"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Uint,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hemicube depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: super::gpu::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let ids_view = ids.create_view(&Default::default());
        let depth_view = depth.create_view(&Default::default());

        // -- weights, uploaded once ---------------------------------------
        let weights = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("hemicube weights"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            bytemuck::cast_slice(&delta_form_factors(resolution)),
        );
        let weights_view = weights.create_view(&Default::default());

        // -- accumulation half ---------------------------------------------
        let accum_shader = device.create_shader_module(super::gpu::SHADER_HEMICUBE_ACCUMULATE);
        let accum_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hemicube accumulate"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<AccumParams>() as u64,
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let accum_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("hemicube accumulate"),
                bind_group_layouts: &[Some(&accum_layout)],
                immediate_size: 0,
            });
        let accum_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("hemicube accumulate"),
            layout: Some(&accum_pipeline_layout),
            module: &accum_shader,
            entry_point: Some("cs_accumulate"),
            compilation_options: Default::default(),
            cache: None,
        });
        let accum_params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hemicube accumulate params"),
            size: STRIDE * batch as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            resolution,
            batch,
            render_pipeline,
            render_layout,
            render_params,
            render_bind,
            accum_pipeline,
            accum_layout,
            accum_params,
            ids,
            ids_view,
            depth_view,
            weights_view,
        }
    }

    pub fn resolution(&self) -> u32 {
        self.resolution
    }

    pub fn batch(&self) -> u32 {
        self.batch
    }

    /// View-factor rows for `views`, a flat list of `FACES` view-projection
    /// matrices per hemicube.
    ///
    /// Returns `n_hemicubes * n_facets` values in row-major order: entry
    /// `[i * n_facets + j]` is `VF(hemicube i -> facet j)`.
    pub fn rows(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        meshes: &[super::gpu::MeshBuffer],
        views: &[crate::Mat4],
        n_facets: u32,
    ) -> Vec<f32> {
        let n_cubes = (views.len() as u32) / FACES;
        let mut out = vec![0.0f32; (n_cubes * n_facets) as usize];

        let acc = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hemicube acc"),
            size: (self.batch * n_facets) as u64 * 4,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hemicube readback"),
            size: (self.batch * n_facets) as u64 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let accum_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hemicube accumulate"),
            layout: &self.accum_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.accum_params,
                        offset: 0,
                        size: wgpu::BufferSize::new(std::mem::size_of::<AccumParams>() as u64),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.ids_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.weights_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: acc.as_entire_binding(),
                },
            ],
        });

        let mut done = 0u32;
        while done < n_cubes {
            let this = (n_cubes - done).min(self.batch);

            // Zero the accumulator for the batch rather than reallocating it.
            queue.write_buffer(&acc, 0, &vec![0u8; (this * n_facets) as usize * 4]);

            for slot in 0..this {
                let cube = done + slot;
                for face in 0..FACES {
                    let m = views[(cube * FACES + face) as usize];
                    queue.write_buffer(
                        &self.render_params,
                        STRIDE * (slot * FACES + face) as u64,
                        bytemuck::bytes_of(&RenderParams {
                            view_proj: to_cols_f32(m),
                            facet_offset: 0,
                            _pad: [0; 3],
                        }),
                    );
                }
                queue.write_buffer(
                    &self.accum_params,
                    STRIDE * slot as u64,
                    bytemuck::bytes_of(&AccumParams {
                        row_offset: slot * n_facets,
                        n_facets,
                        width: self.resolution * FACES,
                        height: self.resolution,
                    }),
                );
            }

            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

            for slot in 0..this {
                // One render pass per hemicube, five viewports inside it, so
                // the atlas is cleared once rather than five times.
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("hemicube"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &self.ids_view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: Some(
                            wgpu::RenderPassDepthStencilAttachment {
                                view: &self.depth_view,
                                depth_ops: Some(wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(1.0),
                                    store: wgpu::StoreOp::Store,
                                }),
                                stencil_ops: None,
                            },
                        ),
                        ..Default::default()
                    });
                    pass.set_pipeline(&self.render_pipeline);
                    for face in 0..FACES {
                        let x = (face * self.resolution) as f32;
                        pass.set_viewport(
                            x,
                            0.0,
                            self.resolution as f32,
                            self.resolution as f32,
                            0.0,
                            1.0,
                        );
                        pass.set_bind_group(
                            0,
                            &self.render_bind,
                            &[(STRIDE * (slot * FACES + face) as u64) as u32],
                        );
                        for mesh in meshes {
                            if mesh.is_flat {
                                mesh.render(&mut pass);
                            }
                        }
                    }
                }
                {
                    let mut pass =
                        encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                            label: Some("hemicube accumulate"),
                            timestamp_writes: None,
                        });
                    pass.set_pipeline(&self.accum_pipeline);
                    pass.set_bind_group(0, &accum_bind, &[(STRIDE * slot as u64) as u32]);
                    pass.dispatch_workgroups(
                        (self.resolution * FACES).div_ceil(8),
                        self.resolution.div_ceil(8),
                        1,
                    );
                }
            }

            encoder.copy_buffer_to_buffer(
                &acc,
                0,
                &staging,
                0,
                (this * n_facets) as u64 * 4,
            );
            queue.submit(Some(encoder.finish()));

            // One readback for the whole batch, rather than one per face as
            // the Python prototype did.
            let slice = staging.slice(..(this * n_facets) as u64 * 4);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                })
                .unwrap();
            {
                let data = slice.get_mapped_range();
                let fixed: &[u32] = bytemuck::cast_slice(&data);
                for k in 0..(this * n_facets) as usize {
                    out[(done * n_facets) as usize + k] = fixed[k] as f32 / FIXED_SCALE;
                }
            }
            staging.unmap();

            done += this;
        }

        out
    }
}

fn to_cols_f32(m: crate::Mat4) -> [[f32; 4]; 4] {
    let c = m.to_cols_array_2d();
    [
        [c[0][0] as f32, c[0][1] as f32, c[0][2] as f32, c[0][3] as f32],
        [c[1][0] as f32, c[1][1] as f32, c[1][2] as f32, c[1][3] as f32],
        [c[2][0] as f32, c[2][1] as f32, c[2][2] as f32, c[2][3] as f32],
        [c[3][0] as f32, c[3][1] as f32, c[3][2] as f32, c[3][3] as f32],
    ]
}

/// Delta form factors for the atlas, laid out as five faces side by side.
///
/// With the hemicube half-width 1 and the facet at the origin:
///
/// ```text
/// top  face, z = 1, pixel at (x, y):  dF = da / (pi (x^2 + y^2 + 1)^2)
/// side face, y = 1, pixel at (x, z):  dF = z da / (pi (x^2 + z^2 + 1)^2)
/// ```
///
/// Only the half of a side face above the facet plane contributes; the rest
/// looks below the horizon and is zeroed. The whole set sums to 1, which is
/// the first thing to check of any implementation -- if the weights do not
/// close, nothing downstream means anything.
pub fn delta_form_factors(resolution: u32) -> Vec<f32> {
    let n = resolution as usize;
    let da = (2.0 / resolution as f32).powi(2);
    let coord = |i: usize| (i as f32 + 0.5) / resolution as f32 * 2.0 - 1.0;

    let mut out = vec![0.0f32; n * n * FACES as usize];
    for row in 0..n {
        for face in 0..FACES as usize {
            for col in 0..n {
                let x = coord(col);
                // Row 0 is the top of the frame, which is +up in view space.
                let v = -coord(row);
                let w = if face == 0 {
                    da / (std::f32::consts::PI * (x * x + v * v + 1.0).powi(2))
                } else if v > 0.0 {
                    v * da / (std::f32::consts::PI * (x * x + v * v + 1.0).powi(2))
                } else {
                    0.0
                };
                out[row * n * FACES as usize + face * n + col] = w;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The weights must sum to 1 over the hemicube, and converge with
    /// resolution. This is pure arithmetic, so it runs without a GPU.
    #[test]
    fn delta_form_factors_close_to_unity() {
        let mut previous = f32::INFINITY;
        for res in [32u32, 64, 128, 256] {
            let error = (delta_form_factors(res).iter().sum::<f32>() - 1.0).abs();
            assert!(error < 1e-3, "closure error {error} at resolution {res}");
            assert!(
                error < previous,
                "closure error did not improve: {error} at {res} against {previous}"
            );
            previous = error;
        }
    }
}
