use crate::app::gpu;

pub fn create_render_target(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });

    let view = texture.create_view(&Default::default());

    (texture, view)
}

/// Picks a sample count the adapter will actually accept.
///
/// Asking for one it does not support is a panic inside wgpu at pipeline
/// creation, which is a poor way to learn that a machine only does 4x. 4 is
/// guaranteed for every renderable format, so that is the first fallback and
/// 1 the last.
pub fn resolve_samples(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    requested: u32,
) -> u32 {
    let flags = format.guaranteed_format_features(device.features()).flags;
    for n in [requested, 4, 1] {
        if n == 1 {
            return 1;
        }
        if n.is_power_of_two() && n <= 8 && flags.sample_count_supported(n) {
            return n;
        }
    }
    1
}

/// The multisampled colour and depth buffers the main pass draws into when
/// `Config::msaa` is above 1. Neither is ever read back: colour resolves into
/// `render_texture` (the single-sample target that has always been exported
/// and blitted), and depth is discarded at the end of the pass.
struct Msaa {
    _color: wgpu::Texture,
    color_view: wgpu::TextureView,
    _depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
}

impl Msaa {
    fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        samples: u32,
    ) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let make = |format, label| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
        };
        let color = make(format, "msaa color");
        let depth = make(gpu::DEPTH_FORMAT, "msaa depth");
        Self {
            color_view: color.create_view(&Default::default()),
            depth_view: depth.create_view(&Default::default()),
            _color: color,
            _depth: depth,
        }
    }
}

pub struct Pass {
    pub pipeline: gpu::RenderPipeline,
    pub render_texture: wgpu::Texture,
    pub render_view: wgpu::TextureView,
    pub samples: u32,
    msaa: Option<Msaa>,
}

impl Pass {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        config: &crate::app::config::Config,
        layouts: &[Option<&wgpu::BindGroupLayout>],
    ) -> Self {
        // Culling is a main-pass-only decision: the shadow pass deliberately
        // stays unculled so non-closed geometry still casts from whichever
        // side faces the light.
        let cull_mode = if config.render_back_face {
            None
        } else {
            Some(wgpu::Face::Back)
        };

        let samples = resolve_samples(device, format, config.msaa);
        if samples != config.msaa {
            eprintln!(
                "msaa: {}x is not supported here, using {}x",
                config.msaa, samples
            );
        }

        let pipeline = gpu::RenderPipeline::new(
            &device,
            format,
            cull_mode,
            gpu::SHADER_MESH_SHADOW,
            layouts,
            true,
            true,
            samples,
            true,
            wgpu::PrimitiveTopology::TriangleList,
            &[
                crate::mesh::Vertex::geometry_desc(),
                crate::mesh::Vertex::attrib_desc(),
                gpu::MeshBuffer::desc(),
            ],
        );

        let (render_texture, render_view) =
            create_render_target(device, format, config.width, config.height);

        let msaa = (samples > 1)
            .then(|| Msaa::new(device, format, config.width, config.height, samples));

        Self {
            pipeline,
            render_texture,
            render_view,
            samples,
            msaa,
        }
    }

    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) {
        let (render_texture, render_view) = create_render_target(device, format, width, height);
        self.render_texture = render_texture;
        self.render_view = render_view;
        self.msaa = (self.samples > 1)
            .then(|| Msaa::new(device, format, width, height, self.samples));
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        depth_view: &wgpu::TextureView,
        light: &super::light_cube::Pass,
        axes: &super::axes::Pass,
        meshes: &[gpu::MeshBuffer],
        bindings: &super::Bindings,
        config: &crate::app::config::Config,
    ) {
        // With MSAA the pass draws into the multisample buffers and resolves
        // into `render_view` on store, so everything downstream -- the blit to
        // the surface, the HUD overlay, the frame exporter -- keeps reading
        // the same single-sample texture it always did.
        let (color_view, resolve_target, store, depth_view) = match &self.msaa {
            Some(msaa) => (
                &msaa.color_view,
                Some(&self.render_view),
                wgpu::StoreOp::Discard,
                &msaa.depth_view,
            ),
            None => (
                &self.render_view,
                None,
                wgpu::StoreOp::Store,
                depth_view,
            ),
        };

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                depth_slice: None,
                resolve_target,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(config.background),
                    store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });

        render_pass.set_pipeline(&self.pipeline.inner);

        bindings.all(&mut render_pass);

        for mesh in &meshes[1..] {
            mesh.render(&mut render_pass);
        }

        // The light cube is a debug marker, not geometry: it must never
        // affect what the scene looks like. It is drawn *after* the bodies
        // and with depth writes off, so it is hidden by anything in front of
        // it but can never hide anything itself.
        //
        // Drawn first with depth writes on, it occluded the scene wherever it
        // landed in the depth buffer -- on the crater example that removed
        // nearly half the lit surface, and independently of
        // `light_cube_scale`, so shrinking it did not help. The shadow pass
        // already skips it (`meshes[1..]`), so it never cast either; this
        // makes the main pass agree.
        if config.debug_light_cube_show {
            light.render(&mut render_pass, &meshes[0], bindings);
        }

        // After the bodies and, like the light cube, without writing depth:
        // annotation is occluded by what it annotates and never the reverse.
        if config.axes != crate::app::axes::AxesStyle::Off {
            axes.render(&mut render_pass, bindings);
        }
    }
}
