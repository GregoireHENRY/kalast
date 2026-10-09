use crate::app::gpu;

pub struct Pass {
    pub pipeline: gpu::RenderPipeline,
    /// A layer's second depth layer, with the Sun a disc: the pass again
    /// with a fragment stage that drops whatever is not behind the layer's
    /// first surface (`fs_peel`), so the depth test keeps the nearest surface
    /// behind it. Group 3 is the first surface (`peel_layout`).
    peel: wgpu::RenderPipeline,
    pub peel_layout: wgpu::BindGroupLayout,
}

impl Pass {
    pub fn new(
        device: &wgpu::Device,
        config: &crate::app::config::Config,
        layouts: &[Option<&wgpu::BindGroupLayout>],
    ) -> Self {
        // On closed geometry the nearest surface along any ray from the light
        // is a front face, so culling the back faces leaves the depth map
        // identical and halves what the rasteriser is handed: 10.9 -> 8.7 ms
        // a frame on the Didymos pair at 3M facets. `render_back_face` is how
        // a script says its geometry is *not* closed -- open craters, clipped
        // sections, single-sided surfaces -- and then both faces cast, as
        // they always did. Same flag as the main pass, same meaning; a
        // toggle rebuilds both (`Window::rebuild_passes`).
        let cull_mode = if config.shading.render_back_face {
            None
        } else {
            Some(wgpu::Face::Back)
        };

        let pipeline = gpu::RenderPipeline::new(
            &device,
            gpu::DEPTH_FORMAT,
            cull_mode,
            gpu::SHADER_SHADOW,
            &layouts,
            true,
            false,
            1,
            true,
            gpu::SHADOW_COMPARE,
            wgpu::PrimitiveTopology::TriangleList,
            &[
                Some(crate::mesh::Vertex::geometry_desc()),
                Some(gpu::MeshBuffer::desc()),
            ],
        );

        let peel_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow peel"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let mut peel_layouts = layouts.to_vec();
        peel_layouts.resize(3, None);
        peel_layouts.push(Some(&peel_layout));
        let module = device.create_shader_module(gpu::SHADER_SHADOW);
        let peel = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow peel"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                bind_group_layouts: &peel_layouts,
                ..Default::default()
            })),
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
                entry_point: Some("fs_peel"),
                targets: &[],
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
                depth_compare: Some(gpu::SHADOW_COMPARE),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Self { pipeline, peel, peel_layout }
    }

    // pub fn resize(&self) {}

    /// `shadow_meshes` is parallel to `meshes`: where it holds a buffer,
    /// that lower-resolution stand-in is rendered into the shadow map
    /// instead of the full-resolution mesh at the same index.
    /// Renders every occluder into one layer of the shadow array.
    ///
    /// `target` is a single-layer view, not the array view: a render pass
    /// cannot attach an array. The matrix this draws with comes from
    /// `view.light.view_proj`, which the caller rewrites and submits per
    /// layer -- a uniform write is ordered against submits, not against
    /// recording, so all layers in one encoder would share the last value.
    ///
    /// Every body is drawn into every layer, not just the layer's own body.
    /// That is what keeps mutual shadowing: the layer is *aimed* at one body,
    /// but anything between the Sun and it still has to cast. `casters`
    /// narrows that to the bodies whose bounds reach this layer's frustum
    /// (`Window::update`, `aabb_may_hit_frustum`); a body it leaves out would
    /// have had every fragment clipped, so the map comes out the same. `None`
    /// draws every body, indices 1.. of `meshes` (0 is the light cube).
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        meshes: &[gpu::MeshBuffer],
        shadow_meshes: &[Option<gpu::MeshBuffer>],
        casters: Option<&[usize]>,
        bindings: &super::Bindings,
        layer: u32,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
        // The layer's first surface, for its second depth layer; `None` for
        // the first.
        peel: Option<&wgpu::BindGroup>,
    ) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            timestamp_writes: timestamps,
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: target,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(gpu::SHADOW_CLEAR),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });

        bindings.for_shadow(&mut render_pass, layer);
        match peel {
            Some(first) => {
                render_pass.set_pipeline(&self.peel);
                render_pass.set_bind_group(3, Some(first), &[]);
            }
            None => render_pass.set_pipeline(&self.pipeline.inner),
        }

        let everything;
        let casters = match casters {
            Some(c) => c,
            None => {
                everything = (1..meshes.len()).collect::<Vec<_>>();
                &everything
            }
        };
        for &ii in casters {
            let Some(mesh) = meshes.get(ii) else {
                continue;
            };
            let occluder = shadow_meshes
                .get(ii)
                .and_then(|m| m.as_ref())
                .unwrap_or(mesh);
            occluder.render_depth_layer(&mut render_pass, layer as usize);
        }
    }
}
