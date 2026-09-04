//! Per-facet shadow queries, read back from the GPU shadow map.
//!
//! The renderer already builds a depth map from the light's point of view
//! every frame. This runs a compute pass over a body's facets against that
//! same map and reports, per facet, the fraction of its sample points that
//! are occluded -- turning a rendering artefact into a quantity the
//! thermophysical model can use for its surface boundary condition, and the
//! radiance step can use to drop occulted facets.
//!
//! It replaces an O(n_facets) ray/triangle sweep per ray (see
//! `crate::mesh::intersect_mesh`), which for two 3.1M-facet bodies is ~10^13
//! triangle tests. The trade-off is that the shadow map is a sampled
//! approximation: see `notes/2026-08-26_facet_shadow_query/` for the resolution and
//! bias analysis, and for why the point-source Sun assumption -- shared with
//! the ray tracer -- currently dominates the error budget either way.

use wgpu::util::DeviceExt;

/// Sample points per facet: the three vertices plus the centroid. Keep in
/// step with `facet_shadow.wgsl`, which divides by this.
pub const SAMPLES_PER_FACET: u32 = 4;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    model: [[f32; 4]; 4],
    light_view_proj: [[f32; 4]; 4],
    light_pos: [f32; 3],
    n_facets: u32,
    shadow_bias_scale: f32,
    shadow_bias_minimum: f32,
    shadow_normal_offset_scale: f32,
    is_flat: u32,
}

pub struct FacetShadowQuery {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
}

impl FacetShadowQuery {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(super::gpu::SHADER_FACET_SHADOW);

        let storage = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("facet_shadow"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                storage(2, true),
                storage(3, true),
                storage(4, false),
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("facet_shadow"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("facet_shadow"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self { pipeline, layout }
    }

    /// Occluded fraction per facet, in `[0, 1]` quantised to
    /// `1 / SAMPLES_PER_FACET`. Index `i` is facet `i` of `mesh`, matching
    /// the CPU-side `Mesh::facets` ordering.
    ///
    /// Blocking: submits the compute pass and stalls until the readback
    /// completes. Intended to be called once per simulation step, not per
    /// rendered frame.
    pub fn query(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shadow: &super::gpu::Texture,
        // Which layer of the shadow array to read. Each layer is fitted to
        // its own body, so this must be the queried body's layer and must
        // pair with `light_view_proj` -- reading another body's layer gives
        // occlusion for a volume this body is not in.
        shadow_layer: usize,
        mesh: &super::gpu::MeshBuffer,
        model: crate::Mat4,
        light_view_proj: crate::Mat4,
        light_pos: crate::Vec3,
        // This layer's own (normal_offset_scale, bias_scale, bias_minimum).
        // Not the `Globals` scalars: those are fitted to the whole scene,
        // while each shadow layer is fitted to its own body, and the two
        // differ by the ratio of the bodies' sizes. Applying the scene value
        // to a small body pushes the sample further along the normal than the
        // body's own radius, so nothing reads as occluded -- Deimos beside
        // Mars reported 0.55% of facets shadowed where it should be ~46%.
        // This feeds the TPM, so it was wrong physics, not a wrong picture.
        layer_bias: crate::Vec4,
    ) -> Vec<f32> {
        let n_facets = mesh.n_facets();
        if n_facets == 0 {
            return vec![];
        }

        let params = Params {
            model: to_cols_f32(model),
            light_view_proj: to_cols_f32(light_view_proj),
            light_pos: [light_pos.x as f32, light_pos.y as f32, light_pos.z as f32],
            n_facets,
            shadow_bias_scale: layer_bias.y as f32,
            shadow_bias_minimum: layer_bias.z as f32,
            shadow_normal_offset_scale: layer_bias.x as f32,
            is_flat: mesh.is_flat as u32,
        };

        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("facet_shadow_params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let size = (n_facets as usize * std::mem::size_of::<f32>()) as wgpu::BufferAddress;

        let out_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("facet_shadow_out"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let read_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("facet_shadow_read"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("facet_shadow"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    // A single-layer view: the compute shader binds a plain
                    // texture_depth_2d and cannot take the array view.
                    resource: wgpu::BindingResource::TextureView(
                        shadow
                            .layer_views
                            .get(shadow_layer)
                            .unwrap_or(&shadow.view),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: mesh.geometry_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: mesh.index_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: out_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("facet_shadow"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, Some(&bind_group), &[]);
            pass.dispatch_workgroups(n_facets.div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&out_buffer, 0, &read_buffer, 0, size);
        queue.submit(Some(encoder.finish()));

        let slice = read_buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .unwrap();

        let data = slice.get_mapped_range();
        let out: Vec<f32> = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        read_buffer.unmap();

        out
    }
}

fn to_cols_f32(m: crate::Mat4) -> [[f32; 4]; 4] {
    let c = m.to_cols_array();
    let mut out = [[0.0f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            out[i][j] = c[i * 4 + j] as f32;
        }
    }
    out
}
