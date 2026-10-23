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
    /// The shadows are traced this frame (`raytrace`): no penumbra pass.
    pub rays: bool,
    /// The main pass built to trace rays, group 7 bound (`Passes::new`).
    pub traced: bool,
    /// Built single-sampled with the float pipelines, for a reference image,
    /// a camera's response or a stretch (`Passes::new`).
    pub float_on: bool,
    /// A reference image is made (`shadows.method` "reference"), set each
    /// frame (`Window::update`).
    pub reference_on: bool,
    /// A camera's response is applied (`config.response`), set each frame.
    pub response_on: bool,
    /// Its transform, made at the image's size when first applied.
    pub response: Option<crate::app::response::Response>,
    /// The size a response was made for and did not fit the GPU at: drawn
    /// without it, not tried again every frame.
    response_refused: Option<(u32, u32)>,
    /// A stretch shows the image (`config.stretch`), set each frame.
    pub stretch_on: bool,
    /// The image drawn for photometry (`config.photometry`), set each frame;
    /// and this frame measured -- its iteration and a pixel's area -- set by
    /// the window before `render`.
    pub photometry_on: bool,
    pub photometry_tag: Option<(u64, f64)>,
    /// Telescope frames to make of this frame's light, set by the window
    /// before `render`, and their pass, made with the first.
    pub telescope_requests: Vec<crate::app::telescope::Request>,
    pub telescope: Option<crate::app::telescope::Telescope>,
    /// Its passes, made at the image's size when first shown.
    pub stretch: Option<crate::app::stretch::Stretch>,
    /// The reference image's sums, made at the image's size when first
    /// drawn.
    pub reference: Option<crate::app::reference::Reference>,
    pub render: render::Pass,
    pub light_cube: light_cube::Pass,
    pub axes: axes::Pass,
    pub grid: grid::Pass,
    pub gizmo: gizmo::Pass,
    pub colorbar: colorbar::Pass,

    pub depth: depth::Pass,
    pub occlusion: super::occlusion::Occlusion,
    /// Photometry's image and its sums (`config.photometry`), built with the
    /// main pass's layouts while it is on.
    pub photometry: Option<crate::app::photometry::Photometry>,

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
        // The main pass's group 7, the bodies to trace rays against, when it
        // traces them; `None` builds it without (`gpu::shader_for_traced`).
        rays: Option<&wgpu::BindGroupLayout>,
        // A reference image is being made (`shadows.method` "reference"): everything
        // single-sampled -- the image is integrated over its pixels instead
        // -- and the float pipelines its samples are drawn with.
        reference: bool,
        // A camera's response is applied (`config.response`): the image
        // drawn as a reference's sample is, into floats, for it to take.
        response: bool,
        // A stretch shows the image (`config.stretch`): the same, the image
        // in floats for its limits and function.
        stretch: bool,
    ) -> Self {
        let layouts_all = uniforms.layouts_all();
        let bindings = uniforms.bindings(device);
        // Photometry's image is a float frame too: single-sampled, the
        // overlays drawn over it as over the others.
        let float = reference || response || stretch || config.photometry.enabled;

        // Resolved once, so the main pass and the light cube it draws inside
        // cannot disagree about it.
        let samples = if float { 1 } else { msaa.resolve(config.shading.msaa) };

        let penumbra = penumbra::Pass::new(device, config, &uniforms.layouts_shaded());
        // The main pass reads what the penumbra pass found, at group 6.
        let mut shaded = uniforms.layouts_shaded();
        shaded.push(Some(&penumbra.read_layout));
        // And the bodies' acceleration structure at group 7, where it is.
        if let Some(rays) = rays {
            shaded.push(Some(rays));
        }
        let render = render::Pass::new(device, format, config, &shaded, size, samples, float, rays.is_some());
        let photometry = config
            .photometry
            .enabled
            .then(|| crate::app::photometry::Photometry::new(device, config, &shaded, rays.is_some(), format));

        Self {
            shadow: shadow::Pass::new(device, config, &uniforms.layouts_for_shadow()),

            penumbra,
            rays: false,
            traced: rays.is_some(),
            float_on: float,
            reference_on: reference,
            response_on: response,
            response: None,
            response_refused: None,
            stretch_on: stretch,
            stretch: None,
            photometry_on: config.photometry.enabled,
            photometry_tag: None,
            telescope_requests: Vec::new(),
            telescope: None,
            reference: None,
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

            photometry,

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
        if penumbra::wanted(config) && !self.rays {
            self.penumbra.render(device, queue, encoder, meshes, &self.bindings, config, size, view_proj, timer);
        }
        // A reference image: this frame's sample of every pixel added to the
        // sum, the mean as the image, what the main pass draws over the
        // scene drawn over the mean. A camera's response: the image drawn
        // into floats -- the mean, or the frame drawn as a sample is --
        // convolved, then the same over it. A stretch: the image so far, in
        // floats, shown through it.
        let response = self.response_ready(device, size);
        let stretch = self.stretch_ready(device, size);
        // Photometry: the bodies drawn once, for the light curve, and that
        // image the frame -- not while a reference image is summed.
        let photometry = self.photometry_on && !self.reference_on && self.photometry.is_some();
        if self.float_on && (self.reference_on || response || stretch || photometry) {
            if photometry {
                let tag = self.photometry_tag.take();
                // The image shown where the floats go on: a camera's
                // response's frame, a stretch's, or the window's image.
                let response_frame = self.response.as_ref().filter(|_| response).map(|r| &r.frame_view);
                let stretch_frame = self.stretch.as_ref().filter(|_| stretch).map(|s| &s.frame_view);
                let (out, float) = match response_frame.or(stretch_frame) {
                    Some(frame) => (frame, true),
                    None => (&self.render.render_view, false),
                };
                let p = self.photometry.as_mut().expect("checked above");
                p.draw(
                    device,
                    encoder,
                    meshes,
                    &self.bindings,
                    self.penumbra.read_group(),
                    config,
                    size,
                    &self.depth.texture.view,
                    timer.and_then(|t| t.scope(super::gpu_timing::Scope::Render)),
                );
                if let Some((iteration, area)) = tag {
                    p.measure(encoder, iteration, area);
                    // Telescope frames of this frame's light, from its sums
                    // where they are, on the GPU.
                    let requests = std::mem::take(&mut self.telescope_requests);
                    if let (false, Some((tiles, n_tiles))) = (requests.is_empty(), p.tiles()) {
                        let telescope = self.telescope.get_or_insert_with(|| crate::app::telescope::Telescope::new(device));
                        for request in requests {
                            telescope.expose(device, queue, encoder, request, tiles, n_tiles, area);
                        }
                    }
                }
                p.show(queue, encoder, out, float, config);
            } else if self.reference_on {
                let format = self.render.format();
                if self.reference.as_ref().is_none_or(|r| r.size != size) {
                    self.reference = Some(crate::app::reference::Reference::new(device, size, format));
                }
                let reference = self.reference.as_ref().unwrap();
                if !reference.done && reference.samples() < config.reference.max_samples {
                    self.render.render_reference(
                        encoder,
                        &reference.frame_view,
                        &self.depth.texture.view,
                        meshes,
                        &self.bindings,
                        self.penumbra.read_group(),
                        config,
                        timer.and_then(|t| t.scope(super::gpu_timing::Scope::Render)),
                    );
                }
            } else if let Some(target) = self.float_target() {
                self.render.render_reference(
                    encoder,
                    target,
                    &self.depth.texture.view,
                    meshes,
                    &self.bindings,
                    self.penumbra.read_group(),
                    config,
                    timer.and_then(|t| t.scope(super::gpu_timing::Scope::Render)),
                );
            }
            self.finish_float(queue, encoder, meshes, config, annotations);
            return;
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

    /// One band of a frame drawn in bands (`Window::render`): rows
    /// `band.0..band.1` of the bodies, the main pass's or a reference
    /// image's sample. Only for a traced frame, which has no penumbra pass.
    #[allow(clippy::too_many_arguments)]
    pub fn render_band(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        meshes: &[super::gpu::MeshBuffer],
        config: &crate::app::config::Config,
        size: (u32, u32),
        band: (u32, u32),
        first: bool,
    ) {
        let response = self.response_ready(device, size);
        let stretch = self.stretch_ready(device, size);
        if self.float_on && (self.reference_on || response || stretch) {
            let target = if self.reference_on {
                let format = self.render.format();
                if self.reference.as_ref().is_none_or(|r| r.size != size) {
                    self.reference = Some(crate::app::reference::Reference::new(device, size, format));
                }
                let reference = self.reference.as_ref().unwrap();
                (!reference.done && reference.samples() < config.reference.max_samples).then_some(&reference.frame_view)
            } else {
                self.float_target()
            };
            if let Some(target) = target {
                self.render.render_reference_band(
                    encoder,
                    target,
                    &self.depth.texture.view,
                    meshes,
                    &self.bindings,
                    self.penumbra.read_group(),
                    config,
                    band,
                    first,
                );
            }
            return;
        }
        self.render.render_band(
            encoder,
            &self.depth.texture.view,
            meshes,
            &self.bindings,
            self.penumbra.read_group(),
            config,
            band,
            first,
        );
    }

    /// After the last band: the sample summed and the mean shown, for a
    /// reference image; the overlays and the resolve, for the main pass --
    /// what `render` does after the bodies.
    pub fn render_bands_finish(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        meshes: &[super::gpu::MeshBuffer],
        config: &crate::app::config::Config,
        annotations: bool,
    ) {
        if self.float_on && (self.reference_on || self.float_target().is_some()) {
            self.finish_float(queue, encoder, meshes, config, annotations);
            return;
        }
        self.render.render_band_finish(
            encoder,
            &self.depth.texture.view,
            &self.light_cube,
            &self.axes,
            &self.grid,
            &self.gizmo,
            &self.colorbar,
            meshes,
            &self.bindings,
            config,
            annotations,
        );
    }

    /// A camera's response ready at `size`, made if it was not: `false`
    /// where it is off, or does not fit the GPU (said once, by
    /// `Response::new`).
    fn response_ready(&mut self, device: &wgpu::Device, size: (u32, u32)) -> bool {
        if !self.response_on {
            return false;
        }
        let size = (size.0.max(1), size.1.max(1));
        if self.response.as_ref().is_some_and(|r| r.size == size) {
            return true;
        }
        if self.response_refused == Some(size) {
            return false;
        }
        self.response = crate::app::response::Response::new(device, size, self.render.format());
        if self.response.is_none() {
            self.response_refused = Some(size);
        }
        self.response.is_some()
    }

    /// A stretch ready at `size`, made if it was not: `false` where it is
    /// off, its passes let go.
    fn stretch_ready(&mut self, device: &wgpu::Device, size: (u32, u32)) -> bool {
        if !self.stretch_on {
            self.stretch = None;
            return false;
        }
        let size = (size.0.max(1), size.1.max(1));
        if self.stretch.as_ref().is_none_or(|s| s.size != size) {
            self.stretch = Some(crate::app::stretch::Stretch::new(device, size, self.render.format()));
        }
        true
    }

    /// Where a float frame's bodies are drawn without a reference: a camera's
    /// response's image, which a stretch's takes after it, or a stretch's.
    fn float_target(&self) -> Option<&wgpu::TextureView> {
        let response = self.response.as_ref().filter(|_| self.response_on).map(|r| &r.frame_view);
        let stretch = self.stretch.as_ref().filter(|_| self.stretch_on).map(|s| &s.frame_view);
        response.or(stretch)
    }

    /// After a float frame's bodies: a reference's sample summed and its mean
    /// resolved, or the frame as drawn, through the camera's response when it
    /// is on, then through a stretch when one is; then the overlays and the
    /// annotations over it.
    fn finish_float(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        meshes: &[super::gpu::MeshBuffer],
        config: &crate::app::config::Config,
        annotations: bool,
    ) {
        // 1: the frame's floats are the values the image stores, decoded for
        // its target (`Config::stores_values`), which the passes below
        // encode to read and decode to write.
        let srgb_mode = config.stores_values() as u32;
        let exposure = config.light.exposure as f32;
        let response = self.response.as_mut().filter(|_| self.response_on);
        // The image before the overlays: the target, or with a stretch its
        // image in floats, shown through it after.
        let stretch = self.stretch.as_mut().filter(|_| self.stretch_on);
        let (out, float) = match stretch.as_ref() {
            Some(s) => (&s.frame_view, true),
            None => (&self.render.render_view, false),
        };
        if self.reference_on {
            if let Some(reference) = self.reference.as_mut() {
                if !reference.done && reference.samples() < config.reference.max_samples {
                    reference.accumulate(queue, encoder, srgb_mode, exposure);
                }
                match response {
                    Some(response) => {
                        reference.resolve(queue, encoder, &response.frame_view, srgb_mode, exposure, true);
                        // A mean that holds still -- done, at the same
                        // exposure -- keeps its result.
                        let key = {
                            use std::hash::{Hash, Hasher};
                            let mut h = std::collections::hash_map::DefaultHasher::new();
                            (reference.resets, reference.samples(), exposure.to_bits(), srgb_mode).hash(&mut h);
                            h.finish()
                        };
                        response.apply(queue, encoder, &config.response, srgb_mode, Some(key));
                        response.show(encoder, out, float);
                    }
                    None => reference.resolve(queue, encoder, out, srgb_mode, exposure, float),
                }
            }
        } else if let Some(response) = response {
            response.apply(queue, encoder, &config.response, srgb_mode, None);
            response.show(encoder, out, float);
        }
        if let Some(stretch) = stretch {
            stretch.apply(queue, encoder, &config.stretch, srgb_mode);
            stretch.show(encoder, &self.render.render_view);
        }
        self.render.render_reference_overlays(
            encoder,
            &self.depth.texture.view,
            &self.light_cube,
            &self.colorbar,
            meshes,
            &self.bindings,
            config,
        );
        if annotations {
            self.render.render_annotations(
                encoder,
                &self.depth.texture.view,
                &self.axes,
                &self.grid,
                &self.gizmo,
                &self.bindings,
                config,
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
    /// The bodies' acceleration structure for the main pass, at group 7, on
    /// a device that traces rays (`raytrace`); set by the window.
    pub rays: Option<wgpu::BindGroup>,
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
