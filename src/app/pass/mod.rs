pub mod depth;
pub mod light_cube;
pub mod render;
pub mod shadow;

pub struct Passes {
    pub shadow: shadow::Pass,

    pub render: render::Pass,
    pub light_cube: light_cube::Pass,

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

        Self {
            shadow: shadow::Pass::new(device, &uniforms.layouts_for_shadow()),

            render: render::Pass::new(device, format, config, &layouts_all),
            light_cube: light_cube::Pass::new(device, format, &layouts_all),

            depth: depth::Pass::new(device, config.width, config.height, format),

            bindings,
        }
    }

    pub fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        shadow: &super::gpu::Texture,
        meshes: &[super::gpu::MeshBuffer],
        config: &crate::app::config::Config,
    ) {
        self.shadow.render(encoder, shadow, meshes, &self.bindings);

        self.render.render(
            encoder,
            &self.depth.texture.view,
            &mut self.light_cube,
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
}

impl Bindings {
    pub fn all(&self, render_pass: &mut wgpu::RenderPass) {
        render_pass.set_bind_group(0, Some(&self.globals), &[]);
        render_pass.set_bind_group(1, Some(&self.view), &[]);
        render_pass.set_bind_group(2, Some(&self.shadow), &[]);
    }

    pub fn for_shadow(&self, render_pass: &mut wgpu::RenderPass) {
        render_pass.set_bind_group(0, Some(&self.globals), &[]);
        render_pass.set_bind_group(1, Some(&self.view), &[]);
    }
}
