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
        // The non-sRGB twin is allowed as a view, so the editor can sample
        // the stored bytes without the hardware converting them to linear
        // first. See `Editor::draw`.
        view_formats: &[format.remove_srgb_suffix()],
    });

    let view = texture.create_view(&Default::default());

    (texture, view)
}

/// The MSAA sample counts this GPU can draw the main pass with -- the
/// image's format and the depth's both -- and its name, for saying why a
/// count was not taken.
///
/// The GPU's own counts where the device took
/// `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`, else the 1 and 4 WebGPU
/// guarantees for every renderable format. Asked once, when the window is
/// made: before, only the guaranteed counts were looked at, and 2x fell back
/// to 4x on a GPU that has it.
#[derive(Clone, Debug)]
pub struct MsaaSupport {
    pub counts: Vec<u32>,
    pub gpu: String,
}

impl MsaaSupport {
    pub fn of(adapter: &wgpu::Adapter, device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let specific = device
            .features()
            .contains(wgpu::Features::from(wgpu::FeaturesWGPU::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES));
        let flags = |f: wgpu::TextureFormat| {
            if specific {
                adapter.get_texture_format_features(f).flags
            } else {
                f.guaranteed_format_features(device.features()).flags
            }
        };
        let (color, depth) = (flags(format), flags(gpu::DEPTH_FORMAT));
        let counts = [1, 2, 4, 8, 16]
            .into_iter()
            .filter(|&n| n == 1 || (color.sample_count_supported(n) && depth.sample_count_supported(n)))
            .collect();
        let info = adapter.get_info();
        Self { counts, gpu: format!("{} ({:?})", info.name, info.backend) }
    }

    /// The count to draw with for `requested`: itself when the GPU has it,
    /// else the largest it has below, said on the console with why. Asking
    /// wgpu for a count the GPU lacks is a panic at pipeline creation.
    pub fn resolve(&self, requested: u32) -> u32 {
        let wanted = requested.max(1);
        let n = self.counts.iter().copied().filter(|&n| n <= wanted).max().unwrap_or(1);
        if n != wanted {
            let has = self.has();
            if wanted.is_power_of_two() {
                eprintln!("msaa: {wanted}x is not supported by {}, which has {has}: using {n}x", self.gpu);
            } else {
                eprintln!(
                    "msaa: {wanted}x is not a sample count, MSAA takes powers of two -- {} has {has}: using {n}x",
                    self.gpu
                );
            }
        }
        n
    }

    /// `1x, 2x and 4x`.
    fn has(&self) -> String {
        let names: Vec<String> = self.counts.iter().map(|n| format!("{n}x")).collect();
        match names.split_last() {
            Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
            _ => names.join(""),
        }
    }
}

/// The multisampled colour and depth buffers the main pass draws into when
/// `Config::msaa` is above 1. Neither is ever read back: colour resolves into
/// `render_texture` (the single-sample target that has always been exported
/// and blitted), and is kept past the pass only when `render_annotations`
/// is to draw on it afterwards.
struct Msaa {
    _color: wgpu::Texture,
    color_view: wgpu::TextureView,
    _depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    /// The colour samples, for `Resolve` to read.
    samples_group: wgpu::BindGroup,
}

/// `shading.srgb_mode = 1` with MSAA: the samples averaged as the values the
/// image stores, by a pass of its own (`msaa_resolve.wgsl`), instead of the
/// hardware resolve, which averages the light they decode to. In that mode a
/// stored value is the lit value itself, I/F times the exposure, so a pixel
/// part lit and part dark has to store the mean of its samples' values.
struct Resolve {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

impl Resolve {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("msaa resolve"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: true,
                },
                count: None,
            }],
        });
        let shader = device.create_shader_module(gpu::SHADER_MSAA_RESOLVE);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("msaa resolve"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("msaa resolve"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Self { pipeline, layout }
    }
}

impl Msaa {
    fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        samples: u32,
        resolve: &Resolve,
    ) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let make = |format, usage, label| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let color = make(
            format,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            "msaa color",
        );
        let depth = make(gpu::DEPTH_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT, "msaa depth");
        let color_view = color.create_view(&Default::default());
        let samples_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("msaa resolve"),
            layout: &resolve.layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&color_view) }],
        });
        Self {
            color_view,
            depth_view: depth.create_view(&Default::default()),
            _color: color,
            _depth: depth,
            samples_group,
        }
    }
}

pub struct Pass {
    pub pipeline: gpu::RenderPipeline,
    /// The same, built lean (`gpu::shader_for`): a vertex carries its
    /// position alone. For a flat mesh drawn indexed, the wireframe off;
    /// `None` on a device without `primitive_index`, where a flat mesh is
    /// always drawn by its corners.
    lean: Option<gpu::RenderPipeline>,
    /// Whether `pipeline` was built with `@builtin(primitive_index)`; see
    /// `gpu::has_primitive_index`. Read per draw to pick the corners path
    /// where the shader needs it.
    primitive_index: bool,
    /// Whether `pipeline` has the eight bytes of immediates a
    /// level-of-detail draw sets; see `gpu::MeshBuffer::render_shaded`.
    immediates: bool,
    pub render_texture: wgpu::Texture,
    pub render_view: wgpu::TextureView,
    pub samples: u32,
    msaa: Option<Msaa>,
    resolve: Resolve,
    /// The image's format: the stored-value resolve is for an sRGB one.
    format: wgpu::TextureFormat,
}

impl Pass {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        config: &crate::app::config::Config,
        layouts: &[Option<&wgpu::BindGroupLayout>],
        // See `Passes::new`: `config.image.width` is `0` while the image follows
        // the window, so the real size has to be handed in.
        size: (u32, u32),
        // `Passes::new`'s, resolved once for every pipeline in the pass.
        samples: u32,
    ) -> Self {
        // The shadow pass reads the same flag (`shadow::Pass::new`): closed
        // geometry culls its back faces in both, and `render_back_face` turns
        // it off in both so non-closed geometry is seen -- and casts -- from
        // whichever side faces the camera or the light.
        //
        // A mirrored image (`image.flip_x` or `flip_y`, not both) turns every
        // triangle's winding round on screen: the faces toward the camera
        // then read as back ones, and it is the front ones that are culled.
        let mirrored = config.image.flip_x != config.image.flip_y;
        let cull_mode = if config.shading.render_back_face {
            None
        } else if mirrored {
            Some(wgpu::Face::Front)
        } else {
            Some(wgpu::Face::Back)
        };

        // Eight bytes of immediates where the device has them: a
        // level-of-detail draw's first triangle and first skirt vertex
        // (`gpu::LodBuffers`).
        let immediates = gpu::has_immediates(device) && gpu::has_primitive_index(device);
        let build = |lean| {
            gpu::RenderPipeline::with_immediates(
                &device,
                format,
                cull_mode,
                gpu::shader_for(device, &gpu::SHADER_MESH_SHADOW, lean),
                layouts,
                samples,
                &[
                    Some(crate::mesh::Vertex::geometry_desc()),
                    Some(gpu::MeshBuffer::desc()),
                ],
                if immediates { 8 } else { 0 },
            )
        };
        let pipeline = build(false);
        let lean = gpu::has_primitive_index(device).then(|| build(true));

        let (render_texture, render_view) =
            create_render_target(device, format, size.0, size.1);

        let resolve = Resolve::new(device, format);
        let msaa = (samples > 1)
            .then(|| Msaa::new(device, format, size.0, size.1, samples, &resolve));

        Self {
            pipeline,
            lean,
            immediates,
            primitive_index: gpu::has_primitive_index(device),
            render_texture,
            render_view,
            samples,
            msaa,
            resolve,
            format,
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
            .then(|| Msaa::new(device, format, width, height, self.samples, &self.resolve));
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        depth_view: &wgpu::TextureView,
        light: &super::light_cube::Pass,
        axes: &super::axes::Pass,
        grid: &super::grid::Pass,
        gizmo: &super::gizmo::Pass,
        colorbar: &super::colorbar::Pass,
        meshes: &[gpu::MeshBuffer],
        bindings: &super::Bindings,
        // Group 6: what the penumbra pass found (`penumbra::Pass::read_group`).
        penumbra: &wgpu::BindGroup,
        config: &crate::app::config::Config,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
        // Drawn last, inside this pass, so the boxes are tested against a
        // depth buffer every body has finished writing.
        occlusion: Option<&crate::app::occlusion::Occlusion>,
        // The grid, the axes and the gizmo, here; `false` leaves them to
        // `render_annotations`, after the frame exporter has copied the scene.
        annotations: bool,
    ) {
        // With MSAA the pass draws into the multisample buffers and resolves
        // into `render_view` on store, so everything downstream -- the blit to
        // the surface, the HUD overlay, the frame exporter -- keeps reading
        // the same single-sample texture it always did. The samples are kept
        // only for a second pass to draw on.
        let by_hand = self.resolves_by_hand(config);
        let (color_view, resolve_target, store, depth_view) = match &self.msaa {
            Some(msaa) => (
                &msaa.color_view,
                (!by_hand).then_some(&self.render_view),
                if annotations && !by_hand { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store },
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
            timestamp_writes: timestamps,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                depth_slice: None,
                resolve_target,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(config.shading.background),
                    store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(gpu::DEPTH_CLEAR),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: occlusion.map(|o| o.query_set()),
            ..Default::default()
        });

        render_pass.set_pipeline(&self.pipeline.inner);

        bindings.all(&mut render_pass);
        render_pass.set_bind_group(6, Some(penumbra), &[]);

        // Non-indexed only while the wireframe is on (`Window::update` sets
        // `INSTANCE_FLAG_CORNERS` from the same field), or on a device whose
        // shader cannot name the facet any other way.
        let corners = config.wireframe.mode != 0 || !self.primitive_index;
        let mut lean_set = false;
        for mesh in &meshes[1..] {
            let lean = self.lean.as_ref().filter(|_| mesh.is_flat && !corners);
            if lean.is_some() != lean_set {
                render_pass.set_pipeline(&lean.unwrap_or(&self.pipeline).inner);
                lean_set = lean.is_some();
            }
            mesh.render_shaded(&mut render_pass, corners, self.immediates);
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
        if config.light.cube_show {
            light.render(&mut render_pass, &meshes[0], bindings);
        }

        if annotations {
            draw_ground_and_axes(&mut render_pass, axes, grid, bindings, config);
        }

        // Not in the flat mode, where there is no scale to show -- the bar was
        // drawn there all the same, from the last frame that had one.
        if config.colorbar.enabled && config.shading.color_mode != 2 {
            colorbar.render(&mut render_pass, bindings);
        }

        if annotations {
            draw_gizmo(&mut render_pass, gizmo, config);
        }

        // Last, and after the overlays as much as after the bodies -- none of
        // them write depth, so what the boxes are tested against is the
        // geometry and nothing else.
        if let Some(o) = occlusion {
            bindings.for_occlusion(&mut render_pass);
            o.draw(&mut render_pass);
        }
        drop(render_pass);
        if by_hand {
            self.resolve_stored(encoder);
        }
    }

    /// Whether this frame's samples are resolved by `Resolve` rather than by
    /// the hardware: MSAA on, `srgb_mode` 1, an sRGB image.
    fn resolves_by_hand(&self, config: &crate::app::config::Config) -> bool {
        self.msaa.is_some() && config.shading.srgb_mode == 1 && self.format.is_srgb()
    }

    /// The samples into `render_texture`, averaged as stored values.
    fn resolve_stored(&self, encoder: &mut wgpu::CommandEncoder) {
        let Some(msaa) = &self.msaa else { return };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("msaa resolve"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.render_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // Every pixel is written.
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.resolve.pipeline);
        pass.set_bind_group(0, &msaa.samples_group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// The grid, the axes and the gizmo, on top of what `render` drew with
    /// `annotations: false` -- for a frame whose export is to leave them out
    /// (`export.axes`): the exporter copies between the two, so the window
    /// has them and the file does not.
    ///
    /// Loads the colour and depth the first pass stored and resolves again,
    /// so everything reading `render_texture` afterwards sees them. Over the
    /// colour bar rather than under it, the one difference from one pass.
    pub fn render_annotations(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        depth_view: &wgpu::TextureView,
        axes: &super::axes::Pass,
        grid: &super::grid::Pass,
        gizmo: &super::gizmo::Pass,
        bindings: &super::Bindings,
        config: &crate::app::config::Config,
    ) {
        let by_hand = self.resolves_by_hand(config);
        let (color_view, resolve_target, depth_view) = match &self.msaa {
            Some(msaa) => (&msaa.color_view, (!by_hand).then_some(&self.render_view), &msaa.depth_view),
            None => (&self.render_view, None, depth_view),
        };
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("annotations"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                depth_slice: None,
                resolve_target,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: if self.msaa.is_some() && !by_hand { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store },
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        draw_ground_and_axes(&mut render_pass, axes, grid, bindings, config);
        draw_gizmo(&mut render_pass, gizmo, config);
        drop(render_pass);
        if by_hand {
            self.resolve_stored(encoder);
        }
    }
}

/// Ground first, then the annotation that stands on it. Both are tested
/// against the bodies and neither writes depth, so the order between them is
/// only about which is drawn over which -- and, like the light cube, after
/// the bodies: annotation is occluded by what it annotates and never the
/// reverse.
fn draw_ground_and_axes(
    render_pass: &mut wgpu::RenderPass,
    axes: &super::axes::Pass,
    grid: &super::grid::Pass,
    bindings: &super::Bindings,
    config: &crate::app::config::Config,
) {
    if config.axes.style == crate::app::axes::AxesStyle::Blender && config.grid.enabled {
        grid.render(render_pass);
    }
    if config.axes.style != crate::app::axes::AxesStyle::Off {
        axes.render(render_pass, bindings);
    }
}

/// On top of every other overlay. It is a control, not an annotation:
/// something that can be clicked has to be the thing under the pointer, so
/// nothing may be drawn over it.
fn draw_gizmo(render_pass: &mut wgpu::RenderPass, gizmo: &super::gizmo::Pass, config: &crate::app::config::Config) {
    if config.axes.style.has_gizmo() {
        gizmo.render(render_pass);
    }
}

#[cfg(test)]
mod tests {
    use super::MsaaSupport;

    /// A count the GPU has is taken as it is, and one it lacks falls to the
    /// largest it has below: on an Apple M1 Pro, which has 1x, 2x and 4x,
    /// 8x and the counts that are no power of two.
    #[test]
    fn a_count_falls_to_the_largest_the_gpu_has_below() {
        let m1 = MsaaSupport { counts: vec![1, 2, 4], gpu: "Apple M1 Pro (Metal)".into() };
        for (asked, got) in [(0, 1), (1, 1), (2, 2), (3, 2), (4, 4), (5, 4), (7, 4), (8, 4), (16, 4)] {
            assert_eq!(m1.resolve(asked), got, "{asked}x");
        }
        assert_eq!(m1.has(), "1x, 2x and 4x");
    }
}
