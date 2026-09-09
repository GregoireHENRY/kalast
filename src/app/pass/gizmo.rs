use crate::app::gpu;

/// Draws the navigation gizmo: screen-space triangles, rebuilt every frame.
///
/// No bind groups at all. The widget carries no uniform because it has
/// nothing to look up: the CPU has already turned the camera basis into
/// positions, so the vertices are the whole of its state.
pub struct Pass {
    pub pipeline: gpu::RenderPipeline,
    /// Grown rather than reallocated. The widget is a fixed twelve quads, so
    /// this settles after the first frame.
    buffer: Option<wgpu::Buffer>,
    capacity: usize,
    n_vertices: u32,
}

impl Pass {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, samples: u32) -> Self {
        let pipeline = gpu::RenderPipeline::blended(
            device,
            format,
            None,
            gpu::SHADER_GIZMO,
            &[],
            true,
            true,
            samples,
            // An overlay: drawn at the near plane so it is never occluded,
            // and writing no depth so it never occludes.
            false,
            gpu::DEPTH_COMPARE,
            wgpu::PrimitiveTopology::TriangleList,
            &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<crate::app::gizmo::Vertex>()
                    as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x2,
                    1 => Float32x2,
                    2 => Float32x4,
                    3 => Float32,
                    4 => Float32,
                ],
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
        vertices: &[crate::app::gizmo::Vertex],
    ) {
        self.n_vertices = vertices.len() as u32;
        if vertices.is_empty() {
            return;
        }

        let bytes: &[u8] = bytemuck::cast_slice(vertices);
        if self.capacity < vertices.len() {
            self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gizmo"),
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

    pub fn render(&self, render_pass: &mut wgpu::RenderPass) {
        let Some(buffer) = &self.buffer else { return };
        if self.n_vertices == 0 {
            return;
        }
        render_pass.set_pipeline(&self.pipeline.inner);
        render_pass.set_vertex_buffer(0, buffer.slice(..));
        render_pass.draw(0..self.n_vertices, 0..1);
    }
}
