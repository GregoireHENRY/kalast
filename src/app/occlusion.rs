//! Bounding-box occlusion queries, for the Visibility panel.
//!
//! The panel's counts come from `diagnose`, which tests each body's bounding
//! box against the frustum. That answers "could the camera see it", not "did
//! it appear": a body wholly behind another still counts. An occlusion query
//! asks the depth buffer instead.
//!
//! The readback discipline is `gpu_timing.rs`'s, and for the same reason --
//! the result must not be waited for. Counts therefore lag the current frame
//! by one or two, and `Counts::frame` says which one they belong to.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// One query per body. Matches the shadow map's layer count, which is the
/// same limit reached from the other direction.
pub const MAX_BODIES: usize = 8;

/// Dynamic offsets into a uniform buffer must clear 256 bytes, so each body's
/// box gets a slot of that rather than the 32 it needs. The box has to be a
/// dynamic offset rather than a write per body for the reason `pass/shadow.rs`
/// records: a `write_buffer` is ordered against submits, not against
/// recording, so every draw in one encoder would see the last value written.
const STRIDE: u64 = 256;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct BoxParams {
    lo: [f32; 3],
    _pad0: f32,
    hi: [f32; 3],
    _pad1: f32,
}

/// Samples that passed depth, per body, from a finished readback.
#[derive(Debug, Clone, Copy)]
pub struct Counts {
    /// Iteration these belong to, which is not the current one.
    pub frame: usize,
    /// False until the first readback lands, and whenever the queries are off.
    pub valid: bool,
    /// How many bodies the counts cover.
    pub n: usize,
    pub samples: [u64; MAX_BODIES],
    /// Bodies whose box contains the camera. Their query is meaningless --
    /// the box's front faces are behind the near plane and its back faces are
    /// behind the body's own surface, so a visible body reports zero. Treated
    /// as drawn without asking.
    pub camera_inside: [bool; MAX_BODIES],
}

impl Default for Counts {
    fn default() -> Self {
        Self {
            frame: 0,
            valid: false,
            n: 0,
            samples: [0; MAX_BODIES],
            camera_inside: [false; MAX_BODIES],
        }
    }
}

impl Counts {
    /// Whether this body put anything on screen.
    pub fn drawn(&self, body: usize) -> bool {
        body < self.n
            && (self.camera_inside.get(body).copied().unwrap_or(false)
                || self.samples.get(body).copied().unwrap_or(0) > 0)
    }

    /// How many bodies did, over the bodies the counts cover.
    pub fn n_drawn(&self) -> usize {
        (0..self.n).filter(|i| self.drawn(*i)).count()
    }
}

pub struct Occlusion {
    query_set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,

    pipeline: wgpu::RenderPipeline,
    params: wgpu::Buffer,
    params_bind_group: wgpu::BindGroup,

    /// Three stages of the same two numbers, because each belongs to a
    /// different frame. `prepare` stages the frame about to be drawn; `draw`
    /// records what actually went into the query set; the readback started
    /// afterwards refers to that. Resolving from the staged pair instead reads
    /// the *previous* frame's queries with the *current* frame's count, which
    /// is wrong for one frame every time a body is added or removed.
    used: Cell<usize>,
    inside: std::cell::RefCell<[bool; MAX_BODIES]>,
    recorded: Cell<usize>,
    recorded_inside: std::cell::RefCell<[bool; MAX_BODIES]>,
    pending_used: Cell<usize>,
    pending_inside: std::cell::RefCell<[bool; MAX_BODIES]>,

    in_flight: Cell<bool>,
    ready: Arc<AtomicBool>,
    pending_frame: Cell<usize>,
    last: Cell<Counts>,
}

impl Occlusion {
    /// `view_layout` is the renderer's own camera bind group layout, reused so
    /// the boxes are projected with exactly the matrix the frame was drawn
    /// with. `samples` must match the main pass, since the queries share its
    /// depth attachment.
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        view_layout: &wgpu::BindGroupLayout,
        samples: u32,
    ) -> Self {
        let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("occlusion box"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<BoxParams>() as u64
                    ),
                },
                count: None,
            }],
        });

        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("occlusion boxes"),
            size: STRIDE * MAX_BODIES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("occlusion boxes"),
            layout: &params_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &params,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<BoxParams>() as u64),
                }),
            }],
        });

        let shader = device.create_shader_module(super::gpu::SHADER_OCCLUSION);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("occlusion"),
            bind_group_layouts: &[Some(view_layout), Some(&params_layout)],
            ..Default::default()
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("occlusion"),
            layout: Some(&layout),
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
                    // The box must never be seen, only counted.
                    write_mask: wgpu::ColorWrites::empty(),
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                // Unculled: inside a box, culling would leave only faces the
                // camera is behind, and the body would read as occluded.
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: super::gpu::DEPTH_FORMAT,
                // Never writes: this runs after the scene and must not change
                // what anything else sees.
                depth_write_enabled: Some(false),
                // `GreaterEqual`, not `Greater`: the box touches its body at
                // the extremes, and a body exactly filling its box in some
                // view would otherwise fail against its own depth.
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        let size = MAX_BODIES as u64 * wgpu::QUERY_SIZE as u64;

        Self {
            query_set: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("occlusion"),
                ty: wgpu::QueryType::Occlusion,
                count: MAX_BODIES as u32,
            }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("occlusion resolve"),
                size,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            read: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("occlusion readback"),
                size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            pipeline,
            params,
            params_bind_group,
            used: Cell::new(0),
            inside: std::cell::RefCell::new([false; MAX_BODIES]),
            recorded: Cell::new(0),
            recorded_inside: std::cell::RefCell::new([false; MAX_BODIES]),
            pending_used: Cell::new(0),
            pending_inside: std::cell::RefCell::new([false; MAX_BODIES]),
            in_flight: Cell::new(false),
            ready: Arc::new(AtomicBool::new(false)),
            pending_frame: Cell::new(0),
            last: Cell::new(Counts::default()),
        }
    }

    /// Upload this frame's boxes. Called before the pass is recorded, since a
    /// buffer write lands at submit and not where it is issued.
    ///
    /// Boxes past `MAX_BODIES` are dropped rather than wrapping onto another
    /// body's slot; the panel says when that happened.
    pub fn prepare(&self, queue: &wgpu::Queue, boxes: &[crate::mesh::Aabb], camera: crate::Vec3) {
        let n = boxes.len().min(MAX_BODIES);
        self.used.set(n);
        let mut inside = self.inside.borrow_mut();
        *inside = [false; MAX_BODIES];

        for (i, b) in boxes.iter().take(n).enumerate() {
            queue.write_buffer(
                &self.params,
                STRIDE * i as u64,
                bytemuck::bytes_of(&BoxParams {
                    lo: [b.min.x as f32, b.min.y as f32, b.min.z as f32],
                    _pad0: 0.0,
                    hi: [b.max.x as f32, b.max.y as f32, b.max.z as f32],
                    _pad1: 0.0,
                }),
            );
            inside[i] = !b.is_empty()
                && camera.cmpge(b.min).all()
                && camera.cmple(b.max).all();
        }
    }

    /// The set to hand to the render pass this will draw inside.
    pub fn query_set(&self) -> &wgpu::QuerySet {
        &self.query_set
    }

    /// Draw the boxes, one query each.
    ///
    /// Must be the last thing in the pass: the answer is only occlusion
    /// against the finished scene if the scene has finished writing depth.
    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        let n = self.used.get();
        self.recorded.set(n);
        *self.recorded_inside.borrow_mut() = *self.inside.borrow();
        if n == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        for i in 0..n {
            pass.set_bind_group(1, Some(&self.params_bind_group), &[(STRIDE * i as u64) as u32]);
            pass.begin_occlusion_query(i as u32);
            pass.draw(0..36, 0..1);
            pass.end_occlusion_query();
        }
    }

    /// Resolve the previous frame's queries. Recorded at the start of the
    /// next frame, for the reason `GpuTimer::resolve` gives.
    pub fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        if self.in_flight.get() || self.recorded.get() == 0 {
            return;
        }
        encoder.resolve_query_set(&self.query_set, 0..MAX_BODIES as u32, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.read, 0, self.read.size());
    }

    /// Start the readback after the submit. The callback only flips a flag.
    pub fn after_submit(&self, frame: usize) {
        if self.in_flight.get() || self.recorded.get() == 0 {
            return;
        }
        self.in_flight.set(true);
        self.pending_frame.set(frame);
        self.pending_used.set(self.recorded.get());
        *self.pending_inside.borrow_mut() = *self.recorded_inside.borrow();
        self.ready.store(false, Ordering::Release);
        let ready = self.ready.clone();
        self.read
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| {
                if r.is_ok() {
                    ready.store(true, Ordering::Release);
                }
            });
    }

    /// Collect a finished readback if there is one. Never blocks.
    pub fn poll(&self, device: &wgpu::Device) -> Counts {
        if self.in_flight.get() {
            let _ = device.poll(wgpu::PollType::Poll);
            if self.ready.load(Ordering::Acquire) {
                self.collect();
            }
        }
        self.last.get()
    }

    fn collect(&self) {
        {
            let Ok(view) = self.read.slice(..).get_mapped_range() else {
                self.in_flight.set(false);
                return;
            };
            let counts: &[u64] = bytemuck::cast_slice(&view);
            let n = self.pending_used.get();
            let mut c = Counts {
                frame: self.pending_frame.get(),
                valid: true,
                n,
                ..Default::default()
            };
            c.samples[..n].copy_from_slice(&counts[..n]);
            c.camera_inside = *self.pending_inside.borrow();
            self.last.set(c);
        }
        self.read.unmap();
        self.in_flight.set(false);
        self.ready.store(false, Ordering::Release);
    }

    /// Forget any pending result, for when the queries are switched off.
    pub fn clear(&self) {
        self.used.set(0);
        self.recorded.set(0);
        self.last.set(Counts::default());
    }
}
