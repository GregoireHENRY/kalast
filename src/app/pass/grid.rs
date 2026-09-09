use crate::app::gpu;

/// The infinite ground grid: one full-screen triangle, shaded per pixel.
///
/// Its own uniform rather than a field on the shared `View`. Every shader
/// that binds `View` declares its own copy of the struct, and a field added
/// to one and not the others silently shifts everything after it -- the fault
/// behind the oversized light cube and the misplaced one before that. A
/// self-contained buffer for a self-contained pass cannot do that.
pub struct Pass {
    pub pipeline: gpu::RenderPipeline,
    uniform: gpu::UniformBuffer<Uniform>,
    bind_group: wgpu::BindGroup,
}

/// Mirrors `Grid` in `shaders/grid.wgsl` field for field.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniform {
    pub inv_view_proj: [[f32; 4]; 4],
    pub view_proj: [[f32; 4]; 4],

    pub thin: [f32; 4],
    pub thick: [f32; 4],
    pub axis_x: [f32; 4],
    pub axis_y: [f32; 4],

    pub spacing: f32,
    pub width: f32,
    pub major: f32,
    pub fade_near: f32,
    pub fade_far: f32,
    /// The five scalars above land on a 16-byte boundary only with this;
    /// WGSL rounds the struct up either way, and a `Pod` that does not agree
    /// with it reads whatever follows.
    pub _pad: [f32; 3],
}

impl Default for Uniform {
    fn default() -> Self {
        Self {
            inv_view_proj: [[0.0; 4]; 4],
            view_proj: [[0.0; 4]; 4],
            thin: [0.32, 0.32, 0.35, 0.5],
            thick: [0.45, 0.45, 0.5, 0.75],
            // Blender's convention, and the same colours the gizmo already
            // uses for its arrows.
            axis_x: [0.78, 0.24, 0.30, 0.9],
            axis_y: [0.38, 0.66, 0.20, 0.9],
            spacing: 1.0,
            width: 1.0,
            major: 10.0,
            fade_near: 1.0,
            fade_far: 1.0,
            _pad: [0.0; 3],
        }
    }
}

impl Pass {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, samples: u32) -> Self {
        let uniform = gpu::UniformBuffer::new(device, Uniform::default());
        let bind_group = uniform.bind_group(device);

        let pipeline = gpu::RenderPipeline::blended(
            device,
            format,
            None,
            gpu::SHADER_GRID,
            &[Some(&uniform.layout)],
            true,
            true,
            samples,
            // Tested against the scene, never written to. The grid is ground,
            // not geometry: a body standing on it must hide it, and it must
            // not hide anything itself. The depth it is tested with comes
            // from the fragment shader, which computes the real intersection.
            false,
            gpu::DEPTH_COMPARE,
            wgpu::PrimitiveTopology::TriangleList,
            &[],
        );

        Self {
            pipeline,
            uniform,
            bind_group,
        }
    }

    pub fn upload(&mut self, queue: &wgpu::Queue, uniform: Uniform) {
        self.uniform.uniform = uniform;
        queue.write_buffer(
            &self.uniform.buffer,
            0,
            bytemuck::bytes_of(&self.uniform.uniform),
        );
    }

    pub fn render(&self, render_pass: &mut wgpu::RenderPass) {
        render_pass.set_pipeline(&self.pipeline.inner);
        render_pass.set_bind_group(0, Some(&self.bind_group), &[]);
        // Three vertices, no buffer: the triangle is built from the index.
        render_pass.draw(0..3, 0..1);
    }
}
