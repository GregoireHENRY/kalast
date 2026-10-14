//! A stretch (`config.stretch`): the image's values shown as SAO DS9 scales
//! an image -- each value taken from between two limits, black at the low
//! one and white at the high one, through a function: linear, a logarithm, a
//! power, a square root, a square, an inverse hyperbolic sine, a hyperbolic
//! sine, or histogram equalisation, DS9's each made 1 at 1 exactly.
//!
//! On the GPU, every frame (`stretch.wgsl`): the image drawn into floats,
//! after the reference's mean and the camera's response, its values -- a
//! pixel's largest channel, as the image stores it -- counted into a
//! histogram of 32 bins an octave with their least and greatest; the limits
//! then found by one workgroup -- given, the least and greatest, the
//! percentiles from the histogram across their bins, or IRAF's zscale fitted
//! to 1000 of the values, sorted, as astropy's `ZScaleInterval` has it --
//! and for equalisation a second histogram, between the limits, and its
//! running shares. The image is then drawn into the target through it, and
//! the overlays over it. The limits are read back without waiting for them,
//! a frame or so late, for the UI and `sim.stretch_limits`.
//!
//! See `notes/2026-10-14_display_stretch/`.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use crate::app::config::Stretch as Settings;

/// The image's floats: the reference's own sample format.
pub const FORMAT: wgpu::TextureFormat = crate::app::reference::FORMAT;

/// `stretch.wgsl`'s: the histogram's bins, zero and 32 an octave from
/// 2^-40 to 2^24; equalisation's, between the limits.
const LOG_BINS: u64 = 2049;
const EQ_BINS: u64 = 2048;
/// A workgroup's square of pixels.
const TILE: u32 = 64;
/// zscale's samples, astropy's default.
const SAMPLES: u32 = 1000;

#[repr(C)]
#[derive(Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    width: u32,
    height: u32,
    srgb_mode: u32,
    scale: u32,
    limits: u32,
    samples: u32,
    stride: u32,
    _pad0: u32,
    percent: f32,
    low: f32,
    high: f32,
    exponent: f32,
    contrast: f32,
    _pad1: [f32; 3],
}

pub struct Stretch {
    /// The image's size.
    pub size: (u32, u32),
    /// The image in floats, as the main pass's shader writes them: drawn
    /// here, or a reference's mean or a camera's response shown here
    /// (`Passes::finish_float`).
    pub frame_view: wgpu::TextureView,
    _frame: wgpu::Texture,
    params: wgpu::Buffer,
    counts: wgpu::Buffer,
    result: wgpu::Buffer,
    read: wgpu::Buffer,
    group: wgpu::BindGroup,
    gather: wgpu::ComputePipeline,
    limits: wgpu::ComputePipeline,
    equalize: wgpu::ComputePipeline,
    cdf: wgpu::ComputePipeline,
    show: wgpu::RenderPipeline,
    /// What `params` holds.
    written: Option<Params>,
    /// A copy of the limits is being read back, which `read` must not be
    /// written while.
    in_flight: bool,
    /// Its map: 0 waiting, 1 mapped, 2 failed.
    ready: Arc<AtomicU8>,
    /// The limits last read back: low, high, and the image's least and
    /// greatest values.
    pub last: Option<[f32; 4]>,
}

impl Stretch {
    pub fn new(device: &wgpu::Device, size: (u32, u32), target: wgpu::TextureFormat) -> Self {
        let (w, h) = (size.0.max(1), size.1.max(1));
        let frame = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("stretch image"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let frame_view = frame.create_view(&Default::default());
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stretch"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let counts = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stretch counts"),
            size: (4 + LOG_BINS + EQ_BINS) * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let result = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stretch limits"),
            size: (4 + EQ_BINS + 1) * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stretch limits, read"),
            size: 16,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let both = wgpu::ShaderStages::COMPUTE | wgpu::ShaderStages::FRAGMENT;
        let storage = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: both,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("stretch"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: both,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: both,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                storage(2),
                storage(3),
            ],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stretch"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&frame_view) },
                wgpu::BindGroupEntry { binding: 2, resource: counts.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: result.as_entire_binding() },
            ],
        });
        let module = device.create_shader_module(crate::app::gpu::SHADER_STRETCH);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("stretch"),
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
        let show = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("stretch show"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_show"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_show"),
                targets: &[Some(wgpu::ColorTargetState { format: target, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            size: (w, h),
            frame_view,
            _frame: frame,
            params,
            counts,
            result,
            read,
            group,
            gather: compute("cs_gather"),
            limits: compute("cs_limits"),
            equalize: compute("cs_equalize"),
            cdf: compute("cs_cdf"),
            show,
            written: None,
            in_flight: false,
            ready: Arc::new(AtomicU8::new(0)),
            last: None,
        }
    }

    /// The limits found from the image in `frame_view`, and for
    /// equalisation its running shares: the passes recorded, the uniform
    /// written first where the settings changed.
    pub fn apply(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, settings: &Settings, srgb_mode: u32) {
        let (w, h) = self.size;
        let params = Params {
            width: w,
            height: h,
            srgb_mode,
            scale: settings.scale.code(),
            limits: settings.limits.code(),
            samples: SAMPLES,
            // astropy's: every `size / n_samples`-th value.
            stride: ((w as u64 * h as u64) / SAMPLES as u64).max(1) as u32,
            _pad0: 0,
            percent: settings.percent,
            low: settings.low,
            high: settings.high,
            exponent: settings.exponent,
            contrast: settings.contrast,
            _pad1: [0.0; 3],
        };
        if self.written != Some(params) {
            queue.write_buffer(&self.params, 0, bytemuck::bytes_of(&params));
            self.written = Some(params);
        }
        encoder.clear_buffer(&self.counts, 0, None);
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("stretch"), timestamp_writes: None });
        pass.set_bind_group(0, Some(&self.group), &[]);
        let tiles = (w.div_ceil(TILE), h.div_ceil(TILE));
        pass.set_pipeline(&self.gather);
        pass.dispatch_workgroups(tiles.0, tiles.1, 1);
        pass.set_pipeline(&self.limits);
        pass.dispatch_workgroups(1, 1, 1);
        if settings.scale == crate::app::config::StretchScale::Histequ {
            pass.set_pipeline(&self.equalize);
            pass.dispatch_workgroups(tiles.0, tiles.1, 1);
            pass.set_pipeline(&self.cdf);
            pass.dispatch_workgroups(1, 1, 1);
        }
    }

    /// The image through the stretch into `target`.
    pub fn show(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("stretch show"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.show);
        pass.set_bind_group(0, Some(&self.group), &[]);
        pass.draw(0..3, 0..1);
    }

    /// The limits as last read back, a frame or so behind, after a frame's
    /// submit: a finished copy collected, and the next one started. Never
    /// waits.
    pub fn after_frame(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Option<[f32; 4]> {
        if self.in_flight {
            let _ = device.poll(wgpu::PollType::Poll);
            match self.ready.load(Ordering::Acquire) {
                0 => return self.last,
                1 => {
                    if let Ok(view) = self.read.slice(..).get_mapped_range() {
                        let v: &[f32] = bytemuck::cast_slice(&view);
                        self.last = Some([v[0], v[1], v[2], v[3]]);
                    }
                    self.read.unmap();
                }
                _ => {}
            }
            self.in_flight = false;
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("stretch limits, read") });
        encoder.copy_buffer_to_buffer(&self.result, 0, &self.read, 0, 16);
        queue.submit(Some(encoder.finish()));
        self.ready.store(0, Ordering::Release);
        let ready = self.ready.clone();
        self.read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            ready.store(if r.is_ok() { 1 } else { 2 }, Ordering::Release);
        });
        self.in_flight = true;
        self.last
    }
}
