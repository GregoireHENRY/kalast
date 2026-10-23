//! Photometry: the image drawn as the light curve needs it, and its flux,
//! `sim.fluxes()`.
//!
//! With `photometry.enabled` the frame's bodies are drawn once, by the main
//! pass's own shader built for photometry (`PHOTOMETRY` in
//! `mesh_shadow.wgsl`): the frame's poses, its shadow maps or rays, the
//! bodies' scattering laws -- each pixel the I/F the Sun leaves on it, the
//! meshes' own normals (a smooth mesh's interpolated), the full meshes rather
//! than their levels of detail, single-sampled into floats, nothing of the
//! ambient light, the bodies' colours or the wireframe. That image is the frame: shown as the
//! main pass would store it (`show`), the camera's response and the stretch
//! after it as on any float frame, the overlays over it. Each frame that
//! advances the simulation, the same image is summed on the GPU by tiles,
//! the tiles added in f64, and read back a frame or two later without
//! waiting, tagged with the iteration it was drawn at.
//!
//! Off, the bodies are drawn as always: levels of detail, multisampling,
//! smooth normals, 8-bit.
//!
//! # Units
//!
//! `sum(I/F * pixel area) / pi`: an area per steradian per unit irradiance,
//! the scene's units squared -- what `lightcurve::flux` gives, so the two
//! are compared as they are. An orthographic camera's pixels are one area,
//! `(2 side / height)^2`; a perspective camera's are not, and its frames are
//! not measured.
//!
//! # Against `lightcurve::flux`
//!
//! The frame samples each pixel at its centre, the CPU integrates each
//! facet exactly: the two part by what a pixel decides at a limb or a
//! shadow's edge -- noise, falling with the image's size. With
//! `shadows.method = "rays"` and the Sun a point, a pixel's shadow is one
//! exact ray, as the CPU's clipping is exact; the shadow maps add their
//! texels and bias at a shadow's edge.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use super::gpu;

/// The image: a pixel's I/F, one float, -1 where no body is.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;

/// The tiles the GPU sums, a side.
const TILE: u32 = 16;

/// Readbacks in flight at once: a frame's flux comes back a frame or two
/// later, and a light curve wants every frame's.
const RING: usize = 4;

/// One frame's flux.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flux {
    /// The iteration the frame was drawn at: the poses a script set before
    /// the step that drew it.
    pub iteration: u64,
    /// `sum(I/F * pixel area) / pi`, as `lightcurve::flux` has it.
    pub flux: f64,
    /// Lit pixels on the frame's edge: a body the frame cuts, its light
    /// short by what lies outside.
    pub clipped: bool,
}

enum State {
    Free,
    /// Copied into in this frame's encoder, not yet mapped.
    Copied { iteration: u64, area: f64 },
    /// Being mapped: its flag says when.
    Mapping { iteration: u64, area: f64 },
}

struct Slot {
    buffer: wgpu::Buffer,
    state: State,
    /// 0 waiting, 1 mapped, 2 failed.
    ready: Arc<AtomicU8>,
}

/// The image and its sums, at the image's size.
struct Target {
    size: (u32, u32),
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    tiles: wgpu::Buffer,
    tiles_xy: (u32, u32),
    group: wgpu::BindGroup,
    slots: Vec<Slot>,
}

/// What the show pass is told, `Show` in `photometry.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Show {
    light: [f32; 3],
    srgb_mode: u32,
    background: [f32; 4],
    gamma: f32,
    _pad: [f32; 3],
}

pub struct Photometry {
    pipeline: gpu::RenderPipeline,
    /// Built lean for a flat mesh drawn indexed (`render::Pass`'s).
    lean: Option<gpu::RenderPipeline>,
    immediates: bool,
    primitive_index: bool,
    sum: wgpu::ComputePipeline,
    image_layout: wgpu::BindGroupLayout,
    /// The image shown into the window's format, and into a float frame
    /// (a camera's response's or a stretch's).
    show: wgpu::RenderPipeline,
    show_float: wgpu::RenderPipeline,
    show_buffer: wgpu::Buffer,
    show_group: wgpu::BindGroup,
    target: Option<Target>,
    /// Frames drawn while every slot was still being read: not measured.
    pub dropped: u64,
}

impl Photometry {
    /// Its pipelines, with the main pass's `layouts` -- the shaded groups,
    /// the penumbra's at 6, the rays' at 7 when `traced` -- and the window's
    /// `format`, which it is shown into.
    pub fn new(
        device: &wgpu::Device,
        config: &crate::app::config::Config,
        layouts: &[Option<&wgpu::BindGroupLayout>],
        traced: bool,
        format: wgpu::TextureFormat,
    ) -> Self {
        // Culled as the main pass culls, the image's mirror included.
        let mirrored = config.image.flip_x != config.image.flip_y;
        let cull_mode = if config.shading.render_back_face {
            None
        } else if mirrored {
            Some(wgpu::Face::Front)
        } else {
            Some(wgpu::Face::Back)
        };
        let immediates = gpu::has_immediates(device) && gpu::has_primitive_index(device);
        let build = |lean| {
            gpu::RenderPipeline::photometric(
                device,
                FORMAT,
                cull_mode,
                gpu::shader_for_traced(device, &gpu::SHADER_MESH_SHADOW, lean, traced),
                layouts,
                &[Some(crate::mesh::Vertex::geometry_desc()), Some(gpu::MeshBuffer::desc())],
                if immediates { 8 } else { 0 },
            )
        };
        let pipeline = build(false);
        let lean = gpu::has_primitive_index(device).then(|| build(true));

        let image_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("photometry image"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let module = device.create_shader_module(gpu::SHADER_PHOTOMETRY);
        let sum = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("photometry sum"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("photometry sum"),
                bind_group_layouts: &[Some(&image_layout)],
                immediate_size: 0,
            })),
            module: &module,
            entry_point: Some("cs_sum"),
            compilation_options: Default::default(),
            cache: None,
        });

        let show_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("photometry show"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let show_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("photometry show"),
            size: std::mem::size_of::<Show>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let show_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("photometry show"),
            layout: &show_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: show_buffer.as_entire_binding() }],
        });
        let show_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("photometry show"),
            bind_group_layouts: &[Some(&image_layout), Some(&show_layout)],
            immediate_size: 0,
        });
        let make_show = |format: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("photometry show"),
                layout: Some(&show_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_show"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_show"),
                    targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            pipeline,
            lean,
            immediates,
            primitive_index: gpu::has_primitive_index(device),
            sum,
            show: make_show(format),
            show_float: make_show(crate::app::reference::FORMAT),
            show_buffer,
            show_group,
            image_layout,
            target: None,
            dropped: 0,
        }
    }

    /// Its image at `size`, made again when it changes -- the readbacks in
    /// flight with it, whose frames are then not measured.
    fn ready(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        if self.target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let (w, h) = (size.0.max(1), size.1.max(1));
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("photometry"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let tiles_xy = (w.div_ceil(TILE), h.div_ceil(TILE));
        let bytes = (tiles_xy.0 * tiles_xy.1) as u64 * 8;
        let tiles = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("photometry tiles"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("photometry image"),
            layout: &self.image_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: tiles.as_entire_binding() },
            ],
        });
        let slots = (0..RING)
            .map(|_| Slot {
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("photometry, read"),
                    size: bytes,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                state: State::Free,
                ready: Arc::new(AtomicU8::new(0)),
            })
            .collect();
        self.target = Some(Target { size, _texture: texture, view, tiles, tiles_xy, group, slots });
    }

    /// The frame's bodies drawn for photometry into its image, over
    /// `depth_view` -- the window's, which the overlays drawn after test
    /// against.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        meshes: &[gpu::MeshBuffer],
        bindings: &crate::app::pass::Bindings,
        penumbra: &wgpu::BindGroup,
        config: &crate::app::config::Config,
        size: (u32, u32),
        depth_view: &wgpu::TextureView,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        self.ready(device, size);
        let target = self.target.as_ref().expect("made just above");
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("photometry"),
            timestamp_writes: timestamps,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view,
                depth_slice: None,
                resolve_target: None,
                // -1 where no body is: the background, which the sum skips
                // and the show pass fills.
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: -1.0, g: 0.0, b: 0.0, a: 0.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(gpu::DEPTH_CLEAR), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline.inner);
        bindings.all(&mut pass);
        pass.set_bind_group(6, Some(penumbra), &[]);
        if let Some(rays) = &bindings.rays {
            pass.set_bind_group(7, Some(rays), &[]);
        }
        // As the main pass picks them (`render::Pass::render`).
        let corners = config.wireframe.mode != 0 || !self.primitive_index;
        let mut lean_set = false;
        for mesh in &meshes[1..] {
            let lean = self.lean.as_ref().filter(|_| mesh.is_flat && !corners);
            if lean.is_some() != lean_set {
                pass.set_pipeline(&lean.unwrap_or(&self.pipeline).inner);
                lean_set = lean.is_some();
            }
            mesh.render_full(&mut pass, corners, self.immediates);
        }
    }

    /// This frame's image summed and the sum copied to be read, tagged with
    /// `iteration`; `area` a pixel's, the scene's units squared. `false`,
    /// nothing measured, while every readback is still in flight.
    pub fn measure(&mut self, encoder: &mut wgpu::CommandEncoder, iteration: u64, area: f64) -> bool {
        let Some(target) = self.target.as_mut() else {
            return false;
        };
        let Some(slot) = target.slots.iter_mut().find(|s| matches!(s.state, State::Free)) else {
            self.dropped += 1;
            return false;
        };
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("photometry sum"), timestamp_writes: None });
            pass.set_pipeline(&self.sum);
            pass.set_bind_group(0, Some(&target.group), &[]);
            pass.dispatch_workgroups(target.tiles_xy.0, target.tiles_xy.1, 1);
        }
        encoder.copy_buffer_to_buffer(&target.tiles, 0, &slot.buffer, 0, target.tiles.size());
        slot.state = State::Copied { iteration, area };
        true
    }

    /// The image shown into `out` -- the window's image, or with `float` a
    /// camera's response's or a stretch's frame -- as the main pass would
    /// have stored it.
    pub fn show(&self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, out: &wgpu::TextureView, float: bool, config: &crate::app::config::Config) {
        let Some(target) = self.target.as_ref() else {
            return;
        };
        let light = gpu::color_vec3(&config.light.color) * config.light.exposure as crate::Float;
        let b = config.shading.background;
        let show = Show {
            light: [light.x as f32, light.y as f32, light.z as f32],
            srgb_mode: config.shading.srgb_mode.code(),
            background: [b.r as f32, b.g as f32, b.b as f32, b.a as f32],
            gamma: config.shading.gamma.unwrap_or(0.0) as f32,
            _pad: [0.0; 3],
        };
        queue.write_buffer(&self.show_buffer, 0, bytemuck::bytes_of(&show));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("photometry show"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: out,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        pass.set_pipeline(if float { &self.show_float } else { &self.show });
        pass.set_bind_group(0, Some(&target.group), &[]);
        pass.set_bind_group(1, Some(&self.show_group), &[]);
        pass.draw(0..3, 0..1);
    }

    /// After the frame's submit: what it copied mapped, what is mapped
    /// read and freed. Never waits; the fluxes in the order of their
    /// iterations.
    pub fn after_frame(&mut self, device: &wgpu::Device) -> Vec<Flux> {
        let Some(target) = self.target.as_mut() else {
            return Vec::new();
        };
        let _ = device.poll(wgpu::PollType::Poll);
        let mut out = Vec::new();
        for slot in &mut target.slots {
            match slot.state {
                State::Mapping { iteration, area } => match slot.ready.load(Ordering::Acquire) {
                    0 => {}
                    1 => {
                        if let Ok(view) = slot.buffer.slice(..).get_mapped_range() {
                            let tiles: &[[f32; 2]] = bytemuck::cast_slice(&view);
                            let (mut sum, mut edge) = (0.0f64, 0.0f64);
                            for t in tiles {
                                sum += t[0] as f64;
                                edge += t[1] as f64;
                            }
                            out.push(Flux { iteration, flux: sum * area / std::f64::consts::PI, clipped: edge > 0.0 });
                        }
                        slot.buffer.unmap();
                        slot.state = State::Free;
                    }
                    _ => slot.state = State::Free,
                },
                State::Copied { iteration, area } => {
                    slot.ready.store(0, Ordering::Release);
                    let ready = slot.ready.clone();
                    slot.buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                        ready.store(if r.is_ok() { 1 } else { 2 }, Ordering::Release);
                    });
                    slot.state = State::Mapping { iteration, area };
                }
                State::Free => {}
            }
        }
        out.sort_by_key(|f| f.iteration);
        out
    }

    /// This frame's sums -- one pair a tile, the tile's and its edge's --
    /// and how many: what a telescope's frame takes its target's light from
    /// (`app::telescope`), in the same encoder.
    pub fn tiles(&self) -> Option<(&wgpu::Buffer, u32)> {
        self.target.as_ref().map(|t| (&t.tiles, t.tiles_xy.0 * t.tiles_xy.1))
    }

    /// Readbacks still in flight.
    pub fn pending(&self) -> usize {
        self.target.as_ref().map_or(0, |t| t.slots.iter().filter(|s| !matches!(s.state, State::Free)).count())
    }
}
