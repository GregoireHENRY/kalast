use crate::app::gpu;

/// Draws the colour scale: one screen-space quad, no vertex buffer.
///
/// Separate from the HUD because it is geometry, not text, and separate from
/// the axes because it is in screen space rather than world space.
pub struct Pass {
    pub pipeline: gpu::RenderPipeline,
}

impl Pass {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        layouts: &[Option<&wgpu::BindGroupLayout>],
        samples: u32,
    ) -> Self {
        let pipeline = gpu::RenderPipeline::new(
            device,
            format,
            None,
            gpu::SHADER_COLORBAR,
            layouts,
            true,
            true,
            samples,
            // Overlay: never occludes the scene it annotates.
            false,
            wgpu::PrimitiveTopology::TriangleList,
            // The quad comes from the vertex index.
            &[],
        );

        Self { pipeline }
    }

    pub fn render(&self, render_pass: &mut wgpu::RenderPass, bindings: &super::Bindings) {
        render_pass.set_pipeline(&self.pipeline.inner);
        bindings.all(render_pass);
        render_pass.draw(0..6, 0..1);
    }
}
