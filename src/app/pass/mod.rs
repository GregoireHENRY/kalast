pub mod axes;
pub mod depth;
pub mod light_cube;
pub mod render;
pub mod shadow;

pub struct Passes {
    pub shadow: shadow::Pass,

    pub render: render::Pass,
    pub light_cube: light_cube::Pass,
    pub axes: axes::Pass,

    pub depth: depth::Pass,

    pub bindings: Bindings,
}

impl Passes {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        config: &crate::app::config::Config,
        uniforms: &super::uniform::Uniforms,
    ) -> Self {
        let layouts_all = uniforms.layouts_all();
        let bindings = uniforms.bindings(device);

        // Resolved once, so the main pass and the light cube it draws inside
        // cannot disagree about it.
        let samples = render::resolve_samples(device, format, config.msaa);

        Self {
            shadow: shadow::Pass::new(device, &uniforms.layouts_for_shadow()),

            render: render::Pass::new(device, format, config, &layouts_all),
            light_cube: light_cube::Pass::new(device, format, &layouts_all, samples),
            axes: axes::Pass::new(device, format, &layouts_all, samples),

            depth: depth::Pass::new(device, config.width, config.height, format),

            bindings,
        }
    }

    /// One shadow layer. Called once per body, each with its own matrix
    /// already written and submitted; see `shadow::Pass::render`.
    pub fn render_shadow_layer(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        meshes: &[super::gpu::MeshBuffer],
        shadow_meshes: &[Option<super::gpu::MeshBuffer>],
    ) {
        self.shadow
            .render(encoder, target, meshes, shadow_meshes, &self.bindings);
    }

    pub fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        shadow: &super::gpu::Texture,
        meshes: &[super::gpu::MeshBuffer],
        shadow_meshes: &[Option<super::gpu::MeshBuffer>],
        config: &crate::app::config::Config,
    ) {
        self.render.render(
            encoder,
            &self.depth.texture.view,
            &mut self.light_cube,
            &self.axes,
            meshes,
            &self.bindings,
            config,
        );

        if config.debug_depth_show {
            self.depth.render(view, encoder);
        }
    }
}

#[derive(Debug, Clone)]
pub struct Bindings {
    pub globals: wgpu::BindGroup,
    pub view: wgpu::BindGroup,
    pub shadow: wgpu::BindGroup,
    pub colormap: wgpu::BindGroup,
}

impl Bindings {
    pub fn all(&self, render_pass: &mut wgpu::RenderPass) {
        render_pass.set_bind_group(0, Some(&self.globals), &[]);
        render_pass.set_bind_group(1, Some(&self.view), &[]);
        render_pass.set_bind_group(2, Some(&self.shadow), &[]);
        render_pass.set_bind_group(3, Some(&self.colormap), &[]);
    }

    pub fn for_shadow(&self, render_pass: &mut wgpu::RenderPass) {
        render_pass.set_bind_group(0, Some(&self.globals), &[]);
        render_pass.set_bind_group(1, Some(&self.view), &[]);
    }
}
