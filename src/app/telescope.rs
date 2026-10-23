//! A telescope's frames made from the rendered light: `sim.expose(...)`.
//!
//! A script asks for an exposure before the step that draws its frame. That
//! frame, drawn for photometry (`config.photometry`), is summed as always
//! (`app::photometry`), and the sum stays on the GPU: `telescope.wgsl` adds
//! its tiles into the target's rendered light, spreads the target along its
//! trail and the field's stars by the seeing, adds the sky and the dark
//! current, draws each pixel's photon and read noise, and converts to the
//! camera's counts. The frame is read back a frame or two later without
//! waiting and written as FITS on a thread of its own, its header as
//! `telescope::expose`'s, the target's magnitude the rendered one.
//!
//! The frame is set out on the CPU by [`telescope::Plan`] -- the same plan
//! `telescope::expose` makes its frames from -- so the two differ in the
//! draws of their noise alone.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;

use crate::telescope::{self as tel, fits, noise::Rng, Plan};

/// One exposure, asked of the frame drawn at `iteration`.
#[derive(Debug, Clone)]
pub struct Request {
    /// The iteration whose frame makes it: the one the script asked at.
    pub iteration: u64,
    pub path: PathBuf,
    pub telescope: tel::Telescope,
    pub band: tel::Band,
    pub sky: tel::Sky,
    pub effects: tel::Effects,
    pub exposure: tel::Exposure,
    /// The target's place and motion; its brightness is the frame's.
    pub target: tel::Source,
    /// Its distances: from the Sun in au, from the observer in the scene's
    /// unit, km.
    pub r: f64,
    pub delta: f64,
    pub stars: Vec<tel::Source>,
    pub object: String,
}

/// A frame written.
#[derive(Debug, Clone, PartialEq)]
pub struct Exposed {
    pub iteration: u64,
    pub path: PathBuf,
    /// The target's rendered light, `sum(I/F * pixel area) / pi`, and its
    /// magnitude in the band above the atmosphere.
    pub flux: f64,
    pub mag: f64,
}

/// `Frame` in `telescope.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Frame {
    width: u32,
    height: u32,
    n_tiles: u32,
    seed: u32,
    background: f32,
    gain: f32,
    read_noise: f32,
    bias: f32,
    full_well: f32,
    alpha: f32,
    beta: f32,
    flags: u32,
    target_scale: f32,
    area_over_pi: f32,
    _pad: [u32; 2],
}

/// `Source` in `telescope.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuSource {
    x: f32,
    y: f32,
    electrons: f32,
    reach: f32,
    rendered: u32,
    _pad: [u32; 3],
}

/// A frame copied for reading, and what its file needs.
struct Pending {
    request: Request,
    header: fits::Header,
}

enum State {
    Free,
    Copied(Box<Pending>),
    Mapping(Box<Pending>),
}

struct Slot {
    buffer: wgpu::Buffer,
    state: State,
    /// 0 waiting, 1 mapped, 2 failed.
    ready: Arc<AtomicU8>,
}

/// Frames in flight at once.
const RING: usize = 3;

/// `Show` in `telescope_show.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Show {
    frame: [u32; 2],
    image: [u32; 2],
    low: f32,
    high: f32,
    raw: u32,
    _pad: u32,
}

/// The buffers of a frame's size.
struct Buffers {
    size: (usize, usize),
    acc: wgpu::Buffer,
    image: wgpu::Buffer,
    slots: Vec<Slot>,
}

pub struct Telescope {
    layout: wgpu::BindGroupLayout,
    light: wgpu::ComputePipeline,
    splat: wgpu::ComputePipeline,
    read: wgpu::ComputePipeline,
    frame: wgpu::Buffer,
    rendered: wgpu::Buffer,
    sources: Option<(wgpu::Buffer, usize)>,
    buffers: Option<Buffers>,
    /// Files being written.
    writing: Arc<AtomicUsize>,
    /// Exposures asked while every slot was busy: not made.
    pub dropped: u64,
    /// The last frame shown in the window (`telescope.show`): into its
    /// format, and into a float frame for the stretch.
    show_layout: wgpu::BindGroupLayout,
    show: wgpu::RenderPipeline,
    show_float: wgpu::RenderPipeline,
    show_buffer: wgpu::Buffer,
    /// The last frame made: its size, and the counts it is shown between
    /// without a stretch -- 3 sigma under its sky to 20 above.
    last: Option<((usize, usize), f32, f32)>,
}

impl Telescope {
    /// Its pipelines; `format` the window's, which a frame is shown into.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::COMPUTE, ty, count: None };
        let storage = |read_only| wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("telescope"),
            entries: &[
                entry(0, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }),
                entry(1, storage(true)),
                entry(2, storage(false)),
                entry(3, storage(false)),
                entry(4, storage(true)),
                entry(5, storage(false)),
            ],
        });
        let module = device.create_shader_module(super::gpu::SHADER_TELESCOPE);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("telescope"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let compute = |entry_point| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let buffer = |label, size, usage| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage, mapped_at_creation: false });

        let show_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("telescope show"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: storage(true),
                    count: None,
                },
            ],
        });
        let show_module = device.create_shader_module(super::gpu::SHADER_TELESCOPE_SHOW);
        let show_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("telescope show"),
            bind_group_layouts: &[Some(&show_layout)],
            immediate_size: 0,
        });
        let make_show = |format: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("telescope show"),
                layout: Some(&show_pipeline_layout),
                vertex: wgpu::VertexState { module: &show_module, entry_point: Some("vs_show"), buffers: &[], compilation_options: Default::default() },
                fragment: Some(wgpu::FragmentState {
                    module: &show_module,
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
            show: make_show(format),
            show_float: make_show(crate::app::reference::FORMAT),
            show_buffer: buffer("telescope show", std::mem::size_of::<Show>() as u64, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST),
            show_layout,
            last: None,
            light: compute("cs_light"),
            splat: compute("cs_splat"),
            read: compute("cs_read"),
            frame: buffer("telescope frame", std::mem::size_of::<Frame>() as u64, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST),
            rendered: buffer("telescope light", 16, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC),
            layout,
            sources: None,
            buffers: None,
            writing: Arc::new(AtomicUsize::new(0)),
            dropped: 0,
        }
    }

    fn ready(&mut self, device: &wgpu::Device, size: (usize, usize)) {
        if self.buffers.as_ref().is_some_and(|b| b.size == size) {
            return;
        }
        let n = (size.0 * size.1) as u64;
        let buffer = |label, size, usage| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage, mapped_at_creation: false });
        self.buffers = Some(Buffers {
            size,
            acc: buffer("telescope electrons", n * 4, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST),
            image: buffer("telescope image", n * 4, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC),
            slots: (0..RING)
                .map(|_| Slot {
                    buffer: buffer("telescope, read", n * 4 + 16, wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST),
                    state: State::Free,
                    ready: Arc::new(AtomicU8::new(0)),
                })
                .collect(),
        });
    }

    /// The frame of `request` made from this frame's photometry -- its
    /// `tiles`, `n_tiles` of them, and a pixel's `area` -- in `encoder`, and
    /// copied to be read. `false`, nothing made, while every slot is busy.
    #[allow(clippy::too_many_arguments)]
    pub fn expose(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        request: Request,
        tiles: &wgpu::Buffer,
        n_tiles: u32,
        area: f64,
    ) -> bool {
        let tel = &request.telescope;
        let (w, h) = (tel.width, tel.height);
        self.ready(device, (w, h));
        let buffers = self.buffers.as_mut().expect("made just above");
        let Some(slot) = buffers.slots.iter_mut().find(|s| matches!(s.state, State::Free)) else {
            self.dropped += 1;
            return false;
        };

        // Set out as `telescope::expose` sets out its own.
        let mut rng = Rng::new(request.exposure.seed);
        let plan = Plan::new(tel, &request.band, &request.sky, &request.effects, &request.exposure, &request.target, &request.stars, &mut rng);
        let reach = |e: f64| plan.psf.map_or(0.5, |p| p.reach(e, 0.01).clamp(3.0, 400.0));
        let on_frame = |x: f64, y: f64, r: f64| x + r >= 0.0 && y + r >= 0.0 && x - r <= w as f64 - 1.0 && y - r <= h as f64 - 1.0;
        let mut sources: Vec<GpuSource> = plan
            .stars
            .iter()
            .map(|&(x, y, e)| (x, y, e, reach(e)))
            .filter(|&(x, y, _, r)| on_frame(x, y, r))
            .map(|(x, y, e, r)| GpuSource { x: x as f32, y: y as f32, electrons: e as f32, reach: r as f32, rendered: 0, _pad: [0; 3] })
            .collect();
        if let Some((a, b)) = plan.trail {
            // Its points a fifth of a pixel apart, each its share of the
            // light, as `psf::add_trail` has them; their stamps wide enough
            // for a bright target, its light unknown here.
            let length = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
            let n = ((length / 0.2).ceil() as usize).max(1);
            let r = reach(1.0e8);
            for k in 0..n {
                let t = (k as f64 + 0.5) / n as f64;
                let (x, y) = (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
                if on_frame(x, y, r) {
                    sources.push(GpuSource { x: x as f32, y: y as f32, electrons: 1.0 / n as f32, reach: r as f32, rendered: 1, _pad: [0; 3] });
                }
            }
        }
        // The rendered light's electrons: `Band::magnitude` undone,
        // m = m_sun - 2.5 log10(F / (r^2 delta^2)).
        let target_scale = plan.target_zero * 10f64.powf(-0.4 * request.band.sun) / (request.r * request.r * request.delta * request.delta);
        let effects = &request.effects;
        let moffat = plan.psf.unwrap_or(crate::telescope::psf::Moffat { alpha: 1.0, beta: 3.0 });
        let frame = Frame {
            width: w as u32,
            height: h as u32,
            n_tiles,
            seed: (request.exposure.seed ^ (request.exposure.seed >> 32)) as u32,
            background: plan.background as f32,
            gain: tel.gain as f32,
            read_noise: tel.read_noise as f32,
            bias: tel.bias as f32,
            full_well: tel.full_well as f32,
            alpha: moffat.alpha as f32,
            beta: moffat.beta as f32,
            flags: effects.psf as u32
                | (effects.photon_noise as u32) << 1
                | (effects.read_noise as u32) << 2
                | (effects.saturation as u32) << 3
                | (effects.bias as u32) << 4,
            target_scale: target_scale as f32,
            area_over_pi: (area / std::f64::consts::PI) as f32,
            _pad: [0; 2],
        };
        queue.write_buffer(&self.frame, 0, bytemuck::bytes_of(&frame));
        let needed = sources.len().max(1);
        if self.sources.as_ref().is_none_or(|(_, n)| *n < needed) {
            let n = needed.next_power_of_two();
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("telescope sources"),
                size: (n * std::mem::size_of::<GpuSource>()) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.sources = Some((buffer, n));
        }
        let (source_buffer, _) = self.sources.as_ref().unwrap();
        if !sources.is_empty() {
            queue.write_buffer(source_buffer, 0, bytemuck::cast_slice(&sources));
        }
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("telescope"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.frame.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: source_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: buffers.acc.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: buffers.image.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: tiles.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: self.rendered.as_entire_binding() },
            ],
        });
        encoder.clear_buffer(&buffers.acc, 0, None);
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("telescope"), timestamp_writes: None });
            pass.set_bind_group(0, Some(&group), &[]);
            pass.set_pipeline(&self.light);
            pass.dispatch_workgroups(1, 1, 1);
            if !sources.is_empty() {
                pass.set_pipeline(&self.splat);
                pass.dispatch_workgroups(sources.len() as u32, 1, 1);
            }
            pass.set_pipeline(&self.read);
            pass.dispatch_workgroups((w as u32).div_ceil(16), (h as u32).div_ceil(16), 1);
        }
        let bytes = (w * h * 4) as u64;
        encoder.copy_buffer_to_buffer(&buffers.image, 0, &slot.buffer, 0, bytes);
        encoder.copy_buffer_to_buffer(&self.rendered, 0, &slot.buffer, bytes, 16);

        let header = tel::header(tel, &request.band, &request.sky, &request.effects, &request.exposure, &request.target, &request.object, &plan.wcs);
        // Shown without a stretch from 3 sigma under the sky to 20 above it.
        let gain = tel.gain;
        let sky = plan.background / gain + if effects.bias { tel.bias } else { 0.0 };
        let read = if effects.read_noise { tel.read_noise } else { 0.0 };
        let sigma = ((if effects.photon_noise { plan.background } else { 0.0 }) + read * read).sqrt().max(1.0) / gain;
        self.last = Some(((w, h), (sky - 3.0 * sigma) as f32, (sky + 20.0 * sigma) as f32));
        slot.state = State::Copied(Box::new(Pending { request, header }));
        true
    }

    /// Whether a frame has been made, to be shown.
    pub fn has_frame(&self) -> bool {
        self.last.is_some()
    }

    /// The last frame made shown into `out`, an image `size` big: its counts
    /// as they are into a float frame (`float`) for the stretch to show, or
    /// from 3 sigma under its sky to 20 above it, black to white.
    pub fn show(&self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, out: &wgpu::TextureView, float: bool, size: (u32, u32)) {
        let (Some(((w, h), low, high)), Some(buffers)) = (self.last, self.buffers.as_ref()) else {
            return;
        };
        let show = Show { frame: [w as u32, h as u32], image: [size.0.max(1), size.1.max(1)], low, high, raw: float as u32, _pad: 0 };
        queue.write_buffer(&self.show_buffer, 0, bytemuck::bytes_of(&show));
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("telescope show"),
            layout: &self.show_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.show_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: buffers.image.as_entire_binding() },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("telescope show"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: out,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        pass.set_pipeline(if float { &self.show_float } else { &self.show });
        pass.set_bind_group(0, Some(&group), &[]);
        pass.draw(0..3, 0..1);
    }

    /// After the frame's submit: what it copied mapped, what is mapped read
    /// and written as FITS on a thread of its own. Never waits.
    pub fn after_frame(&mut self, device: &wgpu::Device) -> Vec<Exposed> {
        let Some(buffers) = self.buffers.as_mut() else {
            return Vec::new();
        };
        let _ = device.poll(wgpu::PollType::Poll);
        let (w, h) = buffers.size;
        let mut out = Vec::new();
        for slot in &mut buffers.slots {
            match std::mem::replace(&mut slot.state, State::Free) {
                State::Copied(pending) => {
                    slot.ready.store(0, Ordering::Release);
                    let ready = slot.ready.clone();
                    slot.buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                        ready.store(if r.is_ok() { 1 } else { 2 }, Ordering::Release);
                    });
                    slot.state = State::Mapping(pending);
                }
                State::Mapping(pending) => match slot.ready.load(Ordering::Acquire) {
                    0 => slot.state = State::Mapping(pending),
                    1 => {
                        let read = slot.buffer.slice(..).get_mapped_range().map(|view| {
                            let floats: &[f32] = bytemuck::cast_slice(&view);
                            (floats[..w * h].to_vec(), floats[w * h] as f64)
                        });
                        slot.buffer.unmap();
                        let Ok((data, flux)) = read else {
                            continue;
                        };
                        let Pending { request, mut header } = *pending;
                        let mag = request.band.magnitude(flux, request.r, request.delta);
                        header.set("TARGMAG", mag, "its rendered magnitude above the atmosphere");
                        header.set("TARGFLUX", flux, "[km2/sr] its rendered light, the frame's sum");
                        out.push(Exposed { iteration: request.iteration, path: request.path.clone(), flux, mag });
                        let writing = self.writing.clone();
                        writing.fetch_add(1, Ordering::AcqRel);
                        let path = request.path;
                        std::thread::spawn(move || {
                            if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
                                let _ = std::fs::create_dir_all(dir);
                            }
                            if let Err(e) = fits::write(&path, w, h, &data, &header, fits::Storage::U16) {
                                eprintln!("telescope: {}: {e}", path.display());
                            }
                            writing.fetch_sub(1, Ordering::AcqRel);
                        });
                    }
                    _ => {}
                },
                State::Free => {}
            }
        }
        out.sort_by_key(|e| e.iteration);
        out
    }

    /// Frames being made, read or written.
    pub fn pending(&self) -> usize {
        let slots = self.buffers.as_ref().map_or(0, |b| b.slots.iter().filter(|s| !matches!(s.state, State::Free)).count());
        slots + self.writing.load(Ordering::Acquire)
    }
}
