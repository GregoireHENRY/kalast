//! Per-pass GPU timings, from timestamp queries.
//!
//! Everything timed here used to be timed by wall clock around a whole frame,
//! which says how long the frame took and nothing about what took it. The
//! question this was written for is in
//! `notes/2026-09-08_step_one_frame_and_a_bad_benchmark.md`: a Rust example
//! sits pinned at the display refresh whatever the workload while the Python
//! module does not, and no amount of wall-clock timing around `step()`
//! distinguishes "the GPU is busy" from "the loop is waiting".
//!
//! The GPU writes a timestamp when a pass begins and another when it ends;
//! the difference, times the queue's tick period, is time actually spent on
//! the device. Two limits come with that, and both are the hardware's:
//!
//! - **Pass boundaries only.** This machine (Apple M1 Pro, Metal) reports
//!   `TIMESTAMP_QUERY` but neither `TIMESTAMP_QUERY_INSIDE_ENCODERS` nor
//!   `..._INSIDE_PASSES`, so a timestamp can be written where a pass starts
//!   and stops and nowhere else. Timing half a pass means splitting the pass.
//! - **Per-pass figures overlap, so they do not sum.** Metal samples at the
//!   pass's vertex-stage start and fragment-stage end, which brackets how
//!   long the pass was *resident* -- queue wait included -- not how much of
//!   the GPU it used. Measured: four bodies report 4.6 ms of shadow passes
//!   inside a frame that took 3.9 ms wall clock. `Timings::span`, first
//!   timestamp to last, is the figure to compare against a frame time.
//! - **The numbers are one frame late.** Reading them back means mapping a
//!   buffer, which is only sound once the GPU has finished with it. Blocking
//!   for that would cost more than it measures, so a frame resolves its
//!   queries and reads whatever an earlier frame left -- a handful of frames
//!   back, in practice. A HUD showing that is not a problem; a benchmark
//!   quoting the last frame of a run would be, so `Timings::frame` carries
//!   the iteration it belongs to.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The passes worth separating. Fixed rather than a map of names because
/// `Diagnostics` is `Copy` and per-frame timings should not allocate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// All shadow layers together -- one pass per body, summed.
    Shadow,
    /// The main pass: bodies, light cube, axes, colour bar.
    Render,
    /// The debug depth view, when it is being drawn.
    Depth,
    /// HUD and facet-label text.
    Text,
    /// The editor's panels.
    Gui,
}

pub const SCOPES: [Scope; 5] = [
    Scope::Shadow,
    Scope::Render,
    Scope::Depth,
    Scope::Text,
    Scope::Gui,
];

impl Scope {
    pub fn index(self) -> usize {
        match self {
            Scope::Shadow => 0,
            Scope::Render => 1,
            Scope::Depth => 2,
            Scope::Text => 3,
            Scope::Gui => 4,
        }
    }

    /// The name a HUD placeholder and the Python dict use.
    pub fn name(self) -> &'static str {
        match self {
            Scope::Shadow => "shadow",
            Scope::Render => "render",
            Scope::Depth => "depth",
            Scope::Text => "text",
            Scope::Gui => "gui",
        }
    }
}

/// One frame's worth, in milliseconds. Zero for a pass that did not run.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Timings {
    pub ms: [f32; SCOPES.len()],
    /// First timestamp to last, across every pass in the frame.
    ///
    /// The one figure to compare against a frame's wall clock. The per-pass
    /// numbers **overlap** -- each is elapsed device time from that pass's
    /// vertex stage starting to its fragment stage ending, which includes
    /// waiting for the GPU to get to it -- so they sum to more than this,
    /// and at four bodies they sum to more than the frame itself.
    pub span: f32,
    /// Iteration these belong to -- not the current one, see the module note.
    pub frame: usize,
    /// False until the first readback lands, so a caller can tell "no
    /// timings yet" from "everything took no time".
    pub valid: bool,
}

impl Timings {
    pub fn get(&self, scope: Scope) -> f32 {
        self.ms[scope.index()]
    }

    /// Sum over the passes. **Not** the GPU's time for the frame, and
    /// usually larger than it -- see `span`, which is.
    pub fn sum(&self) -> f32 {
        self.ms.iter().sum()
    }
}

/// Two timestamps per pass, from a shared pool.
///
/// A pool rather than a slot per scope because two of them repeat within a
/// frame: the shadow map is one pass per body, and the text overlay is drawn
/// up to three times (into the export, into the viewport, onto the
/// swapchain). Slots are handed out in order and each remembers whose it is,
/// so repeats sum instead of overwriting each other.
const N_SLOTS: usize = 16;
const N_QUERIES: u32 = N_SLOTS as u32 * 2;

pub struct GpuTimer {
    query_set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    /// Nanoseconds per tick, from the queue.
    period: f32,
    /// Slots handed out this frame, and to whom. The rest of the set holds
    /// whatever the last frame left and must not be read.
    used: Cell<usize>,
    owner: std::cell::RefCell<[Scope; N_SLOTS]>,
    /// The same two, as they stood when the in-flight readback was started.
    /// The frame being read is not the frame being recorded -- `begin_frame`
    /// has already cleared the live pair by the time the results land.
    pending_used: Cell<usize>,
    pending_owner: std::cell::RefCell<[Scope; N_SLOTS]>,
    /// A readback is in flight, so the buffer cannot be written or mapped.
    in_flight: Cell<bool>,
    ready: Arc<AtomicBool>,
    /// Iteration the in-flight readback belongs to.
    pending_frame: Cell<usize>,
    last: Cell<Timings>,
}

impl GpuTimer {
    /// `None` when the device lacks `TIMESTAMP_QUERY`, which is not an error
    /// -- it is a machine that cannot answer the question.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        if !device
            .features()
            .contains(wgpu::Features::from(wgpu::FeaturesWebGPU::TIMESTAMP_QUERY))
        {
            return None;
        }

        let size = N_QUERIES as u64 * wgpu::QUERY_SIZE as u64;

        Some(Self {
            query_set: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("pass timings"),
                ty: wgpu::QueryType::Timestamp,
                count: N_QUERIES,
            }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("pass timings resolve"),
                size,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            read: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("pass timings readback"),
                size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            period: queue.get_timestamp_period(),
            used: Cell::new(0),
            owner: std::cell::RefCell::new([Scope::Render; N_SLOTS]),
            pending_used: Cell::new(0),
            pending_owner: std::cell::RefCell::new([Scope::Render; N_SLOTS]),
            in_flight: Cell::new(false),
            ready: Arc::new(AtomicBool::new(false)),
            pending_frame: Cell::new(0),
            last: Cell::new(Timings::default()),
        })
    }

    /// Start a frame. Timings from the frame before stay readable until the
    /// next readback lands.
    pub fn begin_frame(&self) {
        self.used.set(0);
    }

    /// Timestamp writes for one pass, to hand to its descriptor.
    ///
    /// Handed out whether or not a readback is in flight. Only `resolve` has
    /// to wait for the buffer: the query set itself is always writable, and
    /// refusing scopes while a map was outstanding meant a frame recorded
    /// only the passes that happened to run after the map landed -- which in
    /// the editor was the egui pass and nothing else, since it is submitted
    /// after `Window::render` returns.
    ///
    /// `None` once the pool is spent, which takes more passes in one frame
    /// than the renderer has.
    pub fn scope(&self, scope: Scope) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let slot = self.used.get();
        if slot >= N_SLOTS {
            return None;
        }
        self.used.set(slot + 1);
        self.owner.borrow_mut()[slot] = scope;
        Some(wgpu::RenderPassTimestampWrites {
            query_set: &self.query_set,
            beginning_of_pass_write_index: Some(slot as u32 * 2),
            end_of_pass_write_index: Some(slot as u32 * 2 + 1),
        })
    }

    /// Resolve into the staging buffer.
    ///
    /// Recorded at the start of the *following* frame, not at the end of the
    /// one being timed: in the editor the egui pass is submitted after
    /// `Window::render` returns, and a resolve inside it would read that
    /// pass's slot before the GPU had written it.
    pub fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        if self.in_flight.get() || self.used.get() == 0 {
            return;
        }
        encoder.resolve_query_set(&self.query_set, 0..N_QUERIES, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.read, 0, self.read.size());
    }

    /// Start the readback, after the frame's submit. Cheap: the callback
    /// only flips a flag, and runs on the render thread inside `poll`.
    pub fn after_submit(&self, frame: usize) {
        if self.in_flight.get() || self.used.get() == 0 {
            return;
        }
        self.in_flight.set(true);
        self.pending_frame.set(frame);
        self.pending_used.set(self.used.get());
        *self.pending_owner.borrow_mut() = *self.owner.borrow();
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

    /// Collect a finished readback, if one is. Never blocks.
    pub fn poll(&self, device: &wgpu::Device) -> Timings {
        if self.in_flight.get() {
            let _ = device.poll(wgpu::PollType::Poll);
            if self.ready.load(Ordering::Acquire) {
                self.collect();
            }
        }
        self.last.get()
    }

    fn collect(&self) {
        let used = self.pending_used.get();
        let owner = self.pending_owner.borrow();
        {
            let Ok(view) = self.read.slice(..).get_mapped_range() else {
                self.in_flight.set(false);
                return;
            };
            let ticks: &[u64] = bytemuck::cast_slice(&view);
            let mut t = Timings {
                frame: self.pending_frame.get(),
                valid: true,
                ..Default::default()
            };
            let (mut first, mut last) = (u64::MAX, 0u64);
            for slot in 0..used {
                let (start, end) = (ticks[slot * 2], ticks[slot * 2 + 1]);
                // A pass the GPU skipped, or a counter that wrapped: either
                // way there is no duration to report, and a negative one
                // would print as a huge positive.
                if end <= start {
                    continue;
                }
                t.ms[owner[slot].index()] += (end - start) as f32 * self.period / 1.0e6;
                first = first.min(start);
                last = last.max(end);
            }
            if last > first {
                t.span = (last - first) as f32 * self.period / 1.0e6;
            }
            self.last.set(t);
        }
        self.read.unmap();
        self.in_flight.set(false);
        self.ready.store(false, Ordering::Release);
    }
}
