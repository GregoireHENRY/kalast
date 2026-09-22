use crate::app::gpu;

pub struct Pass {
    pub pipeline: gpu::RenderPipeline,
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

        Self { pipeline }
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

        render_pass.set_pipeline(&self.pipeline.inner);

        bindings.for_shadow(&mut render_pass, layer);

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
            occluder.render_depth(&mut render_pass);
        }
    }
}
