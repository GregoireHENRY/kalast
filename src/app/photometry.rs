//! The flux of what the camera sees, from the frame it draws: `sim.fluxes()`.
//!
//! Each frame that advances the simulation, with `photometry.enabled`, the
//! bodies are drawn a second time after the image, by the main pass's own
//! shader built for photometry (`PHOTOMETRY` in `mesh_shadow.wgsl`): the same
//! poses, the same shadow maps or rays, the same scattering law -- each
//! pixel the I/F the Sun leaves on it, with its facet's own normal, the full
//! meshes rather than their levels of detail, and nothing of the image's
//! colour, ambient light, exposure or wireframe -- into floats,
//! single-sampled. Summed on the GPU by tiles, the tiles added in f64,
//! read back a frame or two later without waiting, tagged with the
//! iteration the frame was drawn at.
//!
//! # Units
//!
//! `sum(I/F * pixel area) / pi`: an area per steradian per unit
//! irradiance, the scene's units squared -- what `lightcurve::flux` gives,
//! so the two are compared as they are. An orthographic camera's pixels are
//! one area, `(2 side / height)^2`; a perspective camera's are not, and its
//! frames are not measured.
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

/// What the photometry pass draws into: a pixel's I/F, one float.
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

/// The pass's targets and sums, at the image's size.
struct Target {
    size: (u32, u32),
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    _depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    tiles: wgpu::Buffer,
    tiles_xy: (u32, u32),
    group: wgpu::BindGroup,
    slots: Vec<Slot>,
}

pub struct Photometry {
    pipeline: gpu::RenderPipeline,
    /// Built lean for a flat mesh drawn indexed (`render::Pass`'s).
    lean: Option<gpu::RenderPipeline>,
    immediates: bool,
    primitive_index: bool,
    sum: wgpu::ComputePipeline,
    sum_layout: wgpu::BindGroupLayout,
    target: Option<Target>,
    /// Frames drawn while every slot was still being read: not measured.
    pub dropped: u64,
}

impl Photometry {
    /// Its pipelines, with the main pass's `layouts` -- the shaded groups,
    /// the penumbra's at 6, the rays' at 7 when `traced`.
    pub fn new(
        device: &wgpu::Device,
        config: &crate::app::config::Config,
        layouts: &[Option<&wgpu::BindGroupLayout>],
        traced: bool,
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

        let sum_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("photometry sum"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
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
                bind_group_layouts: &[Some(&sum_layout)],
                immediate_size: 0,
            })),
            module: &module,
            entry_point: Some("cs_sum"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            pipeline,
            lean,
            immediates,
            primitive_index: gpu::has_primitive_index(device),
            sum,
            sum_layout,
            target: None,
            dropped: 0,
        }
    }

    /// Its targets at the image's `size`, made again when it changes -- the
    /// readbacks in flight with them, whose frames are then not measured.
    fn ready(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            let (w, h) = (size.0.max(1), size.1.max(1));
            let extent = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
            let make = |format, usage, label| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
            };
            let texture = make(
                FORMAT,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                "photometry",
            );
            let depth = make(gpu::DEPTH_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT, "photometry depth");
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
                label: Some("photometry sum"),
                layout: &self.sum_layout,
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
            self.target = Some(Target {
                size,
                _texture: texture,
                view,
                depth_view: depth.create_view(&Default::default()),
                _depth: depth,
                tiles,
                tiles_xy,
                group,
                slots,
            });
        }
    }

    /// The bodies drawn for photometry and summed, the sum copied to be
    /// read: in the frame's encoder, after the image. `iteration` tags it,
    /// `area` is a pixel's, the scene's units squared. `false`, nothing
    /// drawn, while every readback is still in flight.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        meshes: &[gpu::MeshBuffer],
        bindings: &crate::app::pass::Bindings,
        penumbra: &wgpu::BindGroup,
        config: &crate::app::config::Config,
        size: (u32, u32),
        iteration: u64,
        area: f64,
    ) -> bool {
        self.ready(device, size);
        let Self { pipeline, lean, immediates, primitive_index, sum, target, dropped, .. } = self;
        let target = target.as_mut().expect("made just above");
        let Some(slot) = target.slots.iter_mut().find(|s| matches!(s.state, State::Free)) else {
            *dropped += 1;
            return false;
        };
        let (immediates, primitive_index) = (*immediates, *primitive_index);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("photometry"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(gpu::DEPTH_CLEAR), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&pipeline.inner);
            bindings.all(&mut pass);
            pass.set_bind_group(6, Some(penumbra), &[]);
            if let Some(rays) = &bindings.rays {
                pass.set_bind_group(7, Some(rays), &[]);
            }
            // As the main pass picks them (`render::Pass::render`).
            let corners = config.wireframe.mode != 0 || !primitive_index;
            let mut lean_set = false;
            for mesh in &meshes[1..] {
                let lean = lean.as_ref().filter(|_| mesh.is_flat && !corners);
                if lean.is_some() != lean_set {
                    pass.set_pipeline(&lean.unwrap_or(pipeline).inner);
                    lean_set = lean.is_some();
                }
                mesh.render_full(&mut pass, corners, immediates);
            }
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("photometry sum"), timestamp_writes: None });
            pass.set_pipeline(sum);
            pass.set_bind_group(0, Some(&target.group), &[]);
            pass.dispatch_workgroups(target.tiles_xy.0, target.tiles_xy.1, 1);
        }
        encoder.copy_buffer_to_buffer(&target.tiles, 0, &slot.buffer, 0, target.tiles.size());
        slot.state = State::Copied { iteration, area };
        true
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

    /// Readbacks still in flight.
    pub fn pending(&self) -> usize {
        self.target.as_ref().map_or(0, |t| t.slots.iter().filter(|s| !matches!(s.state, State::Free)).count())
    }
}
