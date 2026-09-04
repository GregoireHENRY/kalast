use crate::app::gpu;

pub struct Pass {
    pub pipeline: gpu::RenderPipeline,
}

impl Pass {
    pub fn new(device: &wgpu::Device, layouts: &[Option<&wgpu::BindGroupLayout>]) -> Self {
        let pipeline = gpu::RenderPipeline::new(
            &device,
            gpu::DEPTH_FORMAT,
            None, // Some(wgpu::Face::Front),
            gpu::SHADER_SHADOW,
            &layouts,
            true,
            false,
            1,
            true,
            wgpu::PrimitiveTopology::TriangleList,
            &[
                crate::mesh::Vertex::geometry_desc(),
                crate::mesh::Vertex::attrib_desc(),
                gpu::MeshBuffer::desc(),
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
    /// but anything between the Sun and it still has to cast.
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        meshes: &[gpu::MeshBuffer],
        shadow_meshes: &[Option<gpu::MeshBuffer>],
        bindings: &super::Bindings,
    ) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: target,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });

        render_pass.set_pipeline(&self.pipeline.inner);

        bindings.for_shadow(&mut render_pass);

        for (ii, mesh) in meshes.iter().enumerate().skip(1) {
            let occluder = shadow_meshes
                .get(ii)
                .and_then(|m| m.as_ref())
                .unwrap_or(mesh);
            occluder.render(&mut render_pass);
        }
    }
}
