use crate::app::gpu;

/// Draws the reference axes: line segments in world space.
///
/// Each segment is an instance of a four-vertex strip, widened on screen by
/// the shader, which blends the line's edges in or cuts them
/// (`axes.antialias`) -- its own antialiasing, whatever the main pass's MSAA.
/// They were `LineList` lines, one pixel wide and smoothed by MSAA alone.
/// Its own pass because it must not write depth: annotation should be
/// occluded by the body it annotates without ever hiding it.
pub struct Pass {
    pub pipeline: gpu::RenderPipeline,
    /// Rebuilt whenever the geometry changes, which is whenever the scene
    /// bounds move. Grown rather than reallocated per frame.
    buffer: Option<wgpu::Buffer>,
    capacity: usize,
    n_vertices: u32,
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
            gpu::SHADER_AXES,
            layouts,
            true,
            true,
            samples,
            // Annotation: occluded by the scene, never occluding it.
            false,
            gpu::DEPTH_COMPARE,
            wgpu::PrimitiveTopology::TriangleStrip,
            // A segment's two vertices read as one instance: the buffer is
            // the line list it always was.
            &[Some(wgpu::VertexBufferLayout {
                array_stride: 2 * std::mem::size_of::<crate::app::axes::LineVertex>()
                    as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Float32x3],
            })],
        );

        Self {
            pipeline,
            buffer: None,
            capacity: 0,
            n_vertices: 0,
        }
    }

    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[crate::app::axes::LineVertex],
    ) {
        self.n_vertices = vertices.len() as u32;
        if vertices.is_empty() {
            return;
        }

        let bytes: &[u8] = bytemuck::cast_slice(vertices);
        if self.capacity < vertices.len() {
            self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("axes"),
                size: bytes.len() as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.capacity = vertices.len();
        }
        if let Some(b) = &self.buffer {
            queue.write_buffer(b, 0, bytes);
        }
    }

    pub fn render(&self, render_pass: &mut wgpu::RenderPass, bindings: &super::Bindings) {
        let Some(buffer) = &self.buffer else { return };
        if self.n_vertices == 0 {
            return;
        }
        render_pass.set_pipeline(&self.pipeline.inner);
        bindings.all(render_pass);
        render_pass.set_vertex_buffer(0, buffer.slice(..));
        render_pass.draw(0..4, 0..self.n_vertices / 2);
    }
}
