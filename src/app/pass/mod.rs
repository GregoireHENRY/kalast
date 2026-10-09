pub mod colorbar;
pub mod axes;
pub mod depth;
pub mod gizmo;
pub mod grid;
pub mod light_cube;
pub mod penumbra;
pub mod render;
pub mod shadow;
pub mod text_layer;

pub struct Passes {
    pub shadow: shadow::Pass,

    /// With the Sun a disc, what each pixel sees of it, before `render`.
    pub penumbra: penumbra::Pass,
    pub render: render::Pass,
    pub light_cube: light_cube::Pass,
    pub axes: axes::Pass,
    pub grid: grid::Pass,
    pub gizmo: gizmo::Pass,
    pub colorbar: colorbar::Pass,

    pub depth: depth::Pass,
    pub occlusion: super::occlusion::Occlusion,

    pub bindings: Bindings,
}

impl Passes {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        config: &crate::app::config::Config,
        uniforms: &super::uniform::Uniforms,
        // The render size, passed in rather than read from the config: since
        // the window and the image became two different sizes,
        // `config.image.width` is `0` whenever the image is following the window.
        size: (u32, u32),
        // The GPU's sample counts, asked when the window was made.
        msaa: &render::MsaaSupport,
    ) -> Self {
        let layouts_all = uniforms.layouts_all();
        let bindings = uniforms.bindings(device);

        // Resolved once, so the main pass and the light cube it draws inside
        // cannot disagree about it.
        let samples = msaa.resolve(config.shading.msaa);

        let penumbra = penumbra::Pass::new(device, config, &uniforms.layouts_shaded());
        // The main pass reads what the penumbra pass found, at group 6.
        let mut shaded = uniforms.layouts_shaded();
        shaded.push(Some(&penumbra.read_layout));
        let render = render::Pass::new(device, format, config, &shaded, size, samples);

        Self {
            shadow: shadow::Pass::new(device, config, &uniforms.layouts_for_shadow()),

            penumbra,
            render,
            light_cube: light_cube::Pass::new(device, format, &layouts_all, samples),
            axes: axes::Pass::new(device, format, &layouts_all, samples),
            grid: grid::Pass::new(device, format, samples),
            gizmo: gizmo::Pass::new(device, format, samples),
            colorbar: colorbar::Pass::new(device, format, &layouts_all, samples),

            depth: depth::Pass::new(device, size.0, size.1, format),

            occlusion: super::occlusion::Occlusion::new(
                device,
                format,
                &uniforms.view.layout,
                samples,
            ),

            bindings,
        }
    }

    /// One shadow layer. Called once per body, each with its own matrix
    /// already written and submitted; see `shadow::Pass::render`. `casters`
    /// is which of `meshes` to draw into it, or every body when `None`.
    pub fn render_shadow_layer(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        meshes: &[super::gpu::MeshBuffer],
        shadow_meshes: &[Option<super::gpu::MeshBuffer>],
        casters: Option<&[usize]>,
        layer: u32,
        timer: Option<&super::gpu_timing::GpuTimer>,
        // The layer's first surface, to draw its second depth layer.
        peel: Option<&wgpu::BindGroup>,
    ) {
        self.shadow.render(
            encoder,
            target,
            meshes,
            shadow_meshes,
            casters,
            &self.bindings,
            layer,
            timer.and_then(|t| t.scope(super::gpu_timing::Scope::Shadow)),
            peel,
        );
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        // Kept in the signature so every pass reads the same, and because
        // the shadow map and its proxy meshes are what a pass added here
        // would want; this one draws neither.
        _shadow: &super::gpu::Texture,
        meshes: &[super::gpu::MeshBuffer],
        _shadow_meshes: &[Option<super::gpu::MeshBuffer>],
        config: &crate::app::config::Config,
        timer: Option<&super::gpu_timing::GpuTimer>,
        // `false`: the grid, axes and gizmo wait for `render_annotations`.
        annotations: bool,
        // The image's size and the camera, which the penumbra pass draws with.
        size: (u32, u32),
        view_proj: crate::Mat4,
    ) {
        if penumbra::wanted(config) {
            self.penumbra.render(device, queue, encoder, meshes, &self.bindings, config, size, view_proj, timer);
        }
        self.render.render(
            encoder,
            &self.depth.texture.view,
            &mut self.light_cube,
            &self.axes,
            &self.grid,
            &self.gizmo,
            &self.colorbar,
            meshes,
            &self.bindings,
            self.penumbra.read_group(),
            config,
            timer.and_then(|t| t.scope(super::gpu_timing::Scope::Render)),
            config.debug.occlusion_queries.then_some(&self.occlusion),
            annotations,
        );

        // Over everything, so after the annotations when those come later.
        if config.debug.depth_show && annotations {
            self.depth.render(
                view,
                encoder,
                timer.and_then(|t| t.scope(super::gpu_timing::Scope::Depth)),
            );
        }
    }

    /// The grid, axes and gizmo that `render(.., annotations: false)` left
    /// out, drawn once the exporter has its copy; see `export.axes`.
    pub fn render_annotations(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        config: &crate::app::config::Config,
        timer: Option<&super::gpu_timing::GpuTimer>,
    ) {
        self.render.render_annotations(
            encoder,
            &self.depth.texture.view,
            &self.axes,
            &self.grid,
            &self.gizmo,
            &self.bindings,
            config,
        );
        if config.debug.depth_show {
            self.depth.render(
                view,
                encoder,
                timer.and_then(|t| t.scope(super::gpu_timing::Scope::Depth)),
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct Bindings {
    pub globals: wgpu::BindGroup,
    pub view: wgpu::BindGroup,
    pub shadow: wgpu::BindGroup,
    pub colormap: wgpu::BindGroup,
    pub bar: wgpu::BindGroup,
    /// Which layer the shadow pass is drawing, picked by dynamic offset.
    pub shadow_layer: wgpu::BindGroup,
    pub layer_stride: u32,
}

impl Bindings {
    pub fn all(&self, render_pass: &mut wgpu::RenderPass) {
        render_pass.set_bind_group(0, Some(&self.globals), &[]);
        render_pass.set_bind_group(1, Some(&self.view), &[]);
        render_pass.set_bind_group(2, Some(&self.shadow), &[]);
        render_pass.set_bind_group(3, Some(&self.colormap), &[]);
        render_pass.set_bind_group(4, Some(&self.bar), &[]);
    }

    /// The occlusion pipeline binds only the camera, at group 0 -- its layout
    /// is `[view, box]`, not the renderer's five. The box itself is set per
    /// body by `Occlusion::draw`.
    pub fn for_occlusion(&self, render_pass: &mut wgpu::RenderPass) {
        render_pass.set_bind_group(0, Some(&self.view), &[]);
    }

    /// `layer` is the shadow map layer being drawn into, and reaches the
    /// shader as a dynamic offset -- which is what lets every layer share one
    /// encoder instead of needing a submit each.
    pub fn for_shadow(&self, render_pass: &mut wgpu::RenderPass, layer: u32) {
        render_pass.set_bind_group(0, Some(&self.globals), &[]);
        render_pass.set_bind_group(1, Some(&self.view), &[]);
        render_pass.set_bind_group(
            2,
            Some(&self.shadow_layer),
            &[layer * self.layer_stride],
        );
    }
}
