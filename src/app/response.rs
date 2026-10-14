//! A camera's response (`config.response`): the image as the camera records
//! it -- its point spread, a Gaussian core and up to two exponential halos,
//! and a smear along a line, the image's motion across the sensor over the
//! exposure -- applied to the image's floats, before the overlays.
//!
//! A convolution by FFT on the GPU (`response.wgsl`): the image, padded with
//! zeros to at least twice its size in powers of two, transformed -- radix-2
//! Stockham, a dispatch a stage, rows then columns, two complex numbers an
//! element, red and green, blue and alpha -- multiplied by the kernel's
//! spectrum, and transformed back. The kernel is laid out on the CPU as
//! `examples/phobos_shadows_fit/hrsc.py`'s response has it, whose fit of the
//! HRSC SRC's its parameters are: each part over the offsets of the image
//! padded to twice its size, normalised there, so that the light a halo
//! spreads past that is put back within it; and transformed on the GPU once
//! for each set of parameters and size.
//!
//! See `notes/2026-10-12_phobos_hrsc_shadows/`, 13 October.

use crate::app::config::Response as Settings;

/// The image's floats: the reference's own sample format.
pub const FORMAT: wgpu::TextureFormat = crate::app::reference::FORMAT;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    width: u32,
    height: u32,
    nx: u32,
    ny: u32,
    p: u32,
    axis: u32,
    inverse: u32,
    srgb_mode: u32,
}

/// Which of the two arrays a stage reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum At {
    A,
    B,
}

impl At {
    fn other(self) -> Self {
        match self {
            At::A => At::B,
            At::B => At::A,
        }
    }
}

pub struct Response {
    /// The image's size.
    pub size: (u32, u32),
    /// The transform's.
    n: (u32, u32),
    /// The image in floats, as the main pass's shader writes them: drawn
    /// here without a reference, a reference's mean resolved here with one
    /// (`Passes::render`).
    pub frame_view: wgpu::TextureView,
    _frame: wgpu::Texture,
    a: wgpu::Buffer,
    b: wgpu::Buffer,
    _spectrum: wgpu::Buffer,
    params: wgpu::Buffer,
    /// The uniforms' spacing, the device's dynamic-offset alignment at least.
    stride: u64,
    /// Bind groups reading `a` and writing `b`, and the other way round.
    ab: wgpu::BindGroup,
    ba: wgpu::BindGroup,
    show_a: wgpu::BindGroup,
    show_b: wgpu::BindGroup,
    load: wgpu::ComputePipeline,
    fft: wgpu::ComputePipeline,
    multiply: wgpu::ComputePipeline,
    kernel_spectrum: wgpu::ComputePipeline,
    show: wgpu::RenderPipeline,
    /// The same into floats, for a stretch to take (`app::stretch`).
    show_float: wgpu::RenderPipeline,
    /// The settings the spectrum is of.
    kernel: Option<[u32; 7]>,
    /// The sRGB mode the uniforms have.
    srgb_mode: Option<u32>,
    /// What the result is of (`apply`'s key): a still image is not
    /// transformed again.
    input: Option<u64>,
    /// Where the result is.
    result: At,
    /// Results made so far: a new one for the simulation to read back.
    pub generation: u64,
}

impl Response {
    /// `None` where the transform's arrays are more than the GPU binds:
    /// said once, and the image drawn without the response.
    pub fn new(device: &wgpu::Device, size: (u32, u32), target: wgpu::TextureFormat) -> Option<Self> {
        let (w, h) = (size.0.max(1), size.1.max(1));
        let (nx, ny) = ((2 * w).next_power_of_two(), (2 * h).next_power_of_two());
        let bytes = nx as u64 * ny as u64 * 16;
        let limits = device.limits();
        if bytes > limits.max_storage_buffer_binding_size as u64 || bytes > limits.max_buffer_size {
            println!(
                "response: a {w} x {h} image wants arrays of {} MB, more than this GPU binds; drawn without it",
                bytes >> 20
            );
            return None;
        }
        let stride = 256u64.max(limits.min_uniform_buffer_offset_alignment as u64);
        let frame = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("response image"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let frame_view = frame.create_view(&Default::default());
        let array = |label| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: bytes,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let (a, b) = (array("response a"), array("response b"));
        let spectrum = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("response spectrum"),
            size: bytes / 2,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let entries = 1 + 2 * (nx.trailing_zeros() + ny.trailing_zeros()) as u64;
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("response stages"),
            size: entries * stride,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform = |visibility| wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Params>() as u64),
            },
            count: None,
        };
        let storage = |binding, read_only, visibility| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let frame_entry = |visibility| wgpu::BindGroupLayoutEntry {
            binding: 4,
            visibility,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let compute = wgpu::ShaderStages::COMPUTE;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("response"),
            entries: &[
                uniform(compute),
                storage(1, true, compute),
                storage(2, false, compute),
                storage(3, false, compute),
                frame_entry(compute),
            ],
        });
        // The image too, for its alpha: coverage, not light, which the
        // response leaves as it was.
        let fragment = wgpu::ShaderStages::FRAGMENT;
        let show_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("response show"),
            entries: &[uniform(fragment), storage(1, true, fragment), frame_entry(fragment)],
        });
        let params_binding = wgpu::BindingResource::Buffer(wgpu::BufferBinding {
            buffer: &params,
            offset: 0,
            size: wgpu::BufferSize::new(std::mem::size_of::<Params>() as u64),
        });
        let group = |from: &wgpu::Buffer, to: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("response"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: params_binding.clone() },
                    wgpu::BindGroupEntry { binding: 1, resource: from.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: to.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: spectrum.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&frame_view) },
                ],
            })
        };
        let show_group = |from: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("response show"),
                layout: &show_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: params_binding.clone() },
                    wgpu::BindGroupEntry { binding: 1, resource: from.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&frame_view) },
                ],
            })
        };
        let (ab, ba) = (group(&a, &b), group(&b, &a));
        let (show_a, show_b) = (show_group(&a), show_group(&b));

        let module = device.create_shader_module(crate::app::gpu::SHADER_RESPONSE);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("response"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let compute_pipeline = |entry_point| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let show_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("response show"),
            bind_group_layouts: &[Some(&show_layout)],
            immediate_size: 0,
        });
        let show_into = |format| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("response show"),
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
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let show = show_into(target);
        let show_float = show_into(FORMAT);
        Some(Self {
            size: (w, h),
            n: (nx, ny),
            frame_view,
            _frame: frame,
            a,
            b,
            _spectrum: spectrum,
            params,
            stride,
            ab,
            ba,
            show_a,
            show_b,
            load: compute_pipeline("cs_load"),
            fft: compute_pipeline("cs_fft"),
            multiply: compute_pipeline("cs_multiply"),
            kernel_spectrum: compute_pipeline("cs_kernel"),
            show,
            show_float,
            kernel: None,
            srgb_mode: None,
            input: None,
            result: At::A,
            generation: 0,
        })
    }

    /// The uniforms' entry for a stage: 0 for the passes that are not one,
    /// then the forward transform's stages, rows then columns, then the
    /// inverse's.
    fn entry(&self, axis: u32, stage: u32, inverse: bool) -> u64 {
        let (lx, ly) = (self.n.0.trailing_zeros(), self.n.1.trailing_zeros());
        let within = if axis == 0 { stage } else { lx + stage };
        1 + (inverse as u32 * (lx + ly) + within) as u64
    }

    fn write_params(&self, queue: &wgpu::Queue, srgb_mode: u32) {
        let (lx, ly) = (self.n.0.trailing_zeros(), self.n.1.trailing_zeros());
        let base = Params {
            width: self.size.0,
            height: self.size.1,
            nx: self.n.0,
            ny: self.n.1,
            p: 0,
            axis: 0,
            inverse: 0,
            srgb_mode,
        };
        let mut data = vec![0u8; self.params.size() as usize];
        let mut put = |entry: u64, p: Params| {
            let at = (entry * self.stride) as usize;
            data[at..at + std::mem::size_of::<Params>()].copy_from_slice(bytemuck::bytes_of(&p));
        };
        put(0, base);
        for inverse in [false, true] {
            for (axis, stages) in [(0u32, lx), (1u32, ly)] {
                for stage in 0..stages {
                    let p = Params { p: 1 << stage, axis, inverse: inverse as u32, ..base };
                    put(self.entry(axis, stage, inverse), p);
                }
            }
        }
        queue.write_buffer(&self.params, 0, &data);
    }

    fn group(&self, from: At) -> &wgpu::BindGroup {
        match from {
            At::A => &self.ab,
            At::B => &self.ba,
        }
    }

    /// The whole transform, rows then columns, from `from`: where it ends.
    fn transform(&self, pass: &mut wgpu::ComputePass, from: At, inverse: bool) -> At {
        pass.set_pipeline(&self.fft);
        let mut at = from;
        for (axis, n, lines) in [(0u32, self.n.0, self.n.1), (1u32, self.n.1, self.n.0)] {
            for stage in 0..n.trailing_zeros() {
                let offset = (self.entry(axis, stage, inverse) * self.stride) as u32;
                pass.set_bind_group(0, Some(self.group(at)), &[offset]);
                // Along the rows a thread a butterfly, down the columns a
                // thread a column (`cs_fft`).
                if axis == 0 {
                    pass.dispatch_workgroups((n / 2).div_ceil(64), lines, 1);
                } else {
                    pass.dispatch_workgroups(lines.div_ceil(64), n / 2, 1);
                }
                at = at.other();
            }
        }
        at
    }

    /// An element-wise pass from `from` into the other array.
    fn each(&self, pass: &mut wgpu::ComputePass, pipeline: &wgpu::ComputePipeline, from: At) -> At {
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, Some(self.group(from)), &[0]);
        pass.dispatch_workgroups(self.n.0.div_ceil(8), self.n.1.div_ceil(8), 1);
        from.other()
    }

    /// The image in `frame_view` convolved: the spectrum made again first
    /// for new settings; nothing done when `key` is the last one's and the
    /// settings are the same -- a still image, its result kept.
    pub fn apply(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, settings: &Settings, srgb_mode: u32, key: Option<u64>) {
        let bits = settings.bits();
        let kernel_new = self.kernel != Some(bits);
        if !kernel_new && key.is_some() && key == self.input && self.srgb_mode == Some(srgb_mode) {
            return;
        }
        if self.srgb_mode != Some(srgb_mode) {
            self.write_params(queue, srgb_mode);
            self.srgb_mode = Some(srgb_mode);
        }
        if kernel_new {
            // Written before the frame's commands run, which transform it
            // before anything else reads `a`.
            let kernel = kernel(self.size, self.n, settings);
            queue.write_buffer(&self.a, 0, bytemuck::cast_slice(&kernel));
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("response"), timestamp_writes: None });
        if kernel_new {
            let at = self.transform(&mut pass, At::A, false);
            self.each(&mut pass, &self.kernel_spectrum, at);
            self.kernel = Some(bits);
        }
        // The image into `a`, read from the frame: through `ba`, whose
        // destination it is.
        pass.set_pipeline(&self.load);
        pass.set_bind_group(0, Some(&self.ba), &[0]);
        pass.dispatch_workgroups(self.n.0.div_ceil(8), self.n.1.div_ceil(8), 1);
        let at = self.transform(&mut pass, At::A, false);
        let at = self.each(&mut pass, &self.multiply, at);
        self.result = self.transform(&mut pass, at, true);
        self.input = key;
        self.generation += 1;
    }

    /// The last result into `target`, the image; `float`, a stretch's image
    /// in floats (`app::stretch`), drawn there as the main pass would.
    pub fn show(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, float: bool) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("response show"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        pass.set_pipeline(if float { &self.show_float } else { &self.show });
        let group = match self.result {
            At::A => &self.show_a,
            At::B => &self.show_b,
        };
        pass.set_bind_group(0, Some(group), &[0]);
        pass.draw(0..3, 0..1);
    }

    /// The last result as floats, three channels a pixel, rows from the
    /// top: the values the image stores. Blocking.
    pub fn read(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<f32> {
        let from = match self.result {
            At::A => &self.a,
            At::B => &self.b,
        };
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("response read"),
            size: from.size(),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("response read") });
        encoder.copy_buffer_to_buffer(from, 0, &read, 0, from.size());
        queue.submit(Some(encoder.finish()));
        let slice = read.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
        let data = slice.get_mapped_range().unwrap();
        let values: &[[f32; 4]] = bytemuck::cast_slice(&data);
        let scale = 1.0 / (self.n.0 as f64 * self.n.1 as f64) as f32;
        let (w, h, nx) = (self.size.0 as usize, self.size.1 as usize, self.n.0 as usize);
        let mut out = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for v in &values[y * nx..y * nx + w] {
                out.extend([v[0] * scale, v[1] * scale, v[2] * scale]);
            }
        }
        drop(data);
        read.unmap();
        out
    }
}

/// The kernel laid out on the transform's grid, offsets wrapped: the point
/// spread in `x`, the smear in `z`, each normalised.
///
/// The point spread is `hrsc.py`'s: over the offsets `-w..w` across and
/// `-h..h` down, each part normalised there -- a Gaussian of `core`, the
/// light the halos leave, and an exponential for each halo, of its share.
/// The smear a segment of length `smear` at `smear_angle`, each pixel the
/// length of it its square holds: along the rows, a box with partial ends,
/// `hrsc.py`'s too.
pub fn kernel(size: (u32, u32), n: (u32, u32), s: &Settings) -> Vec<[f32; 4]> {
    let (w, h) = (size.0 as i64, size.1 as i64);
    let (nx, ny) = (n.0 as i64, n.1 as i64);
    let index = |dx: i64, dy: i64| (dy.rem_euclid(ny) * nx + dx.rem_euclid(nx)) as usize;
    let mut out = vec![[0f32; 4]; (nx * ny) as usize];

    // The shares: the halos', scaled down to 1 together if over it.
    let (mut h1, mut h2) = (s.halo_share.max(0.0) as f64, s.halo2_share.max(0.0) as f64);
    if h1 + h2 > 1.0 {
        let t = h1 + h2;
        h1 /= t;
        h2 /= t;
    }
    let shares = [1.0 - h1 - h2, h1, h2];
    let (sigma, r1, r2) = (s.core as f64, s.halo_scale as f64, s.halo2_scale as f64);
    let parts = |dx: i64, dy: i64| -> [f64; 3] {
        let r = ((dx * dx + dy * dy) as f64).sqrt();
        let origin = (dx == 0 && dy == 0) as u8 as f64;
        [
            if sigma > 0.0 { (-0.5 * (r / sigma).powi(2)).exp() } else { origin },
            if r1 > 0.0 { (-r / r1).exp() } else { origin },
            if r2 > 0.0 { (-r / r2).exp() } else { origin },
        ]
    };
    let mut sums = [0f64; 3];
    for dy in -h..h {
        for dx in -w..w {
            let p = parts(dx, dy);
            for k in 0..3 {
                sums[k] += p[k];
            }
        }
    }
    for dy in -h..h {
        for dx in -w..w {
            let p = parts(dx, dy);
            let v: f64 = (0..3).filter(|&k| shares[k] > 0.0).map(|k| shares[k] * p[k] / sums[k]).sum();
            out[index(dx, dy)][0] = v as f32;
        }
    }

    // The smear.
    let length = s.smear.max(0.0) as f64;
    if length <= 0.0 {
        out[0][2] = 1.0;
        return out;
    }
    let angle = (s.smear_angle as f64).to_radians();
    let (ux, uy) = (angle.cos(), angle.sin());
    let (ax, ay) = (-0.5 * length * ux, -0.5 * length * uy);
    let reach = (0.5 * length).ceil() as i64 + 1;
    let mut cells = Vec::new();
    let mut total = 0.0;
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let held = segment_in_square(ax, ay, length * ux, length * uy, dx as f64, dy as f64) * length;
            if held > 0.0 {
                cells.push((dx, dy, held));
                total += held;
            }
        }
    }
    for (dx, dy, held) in cells {
        out[index(dx, dy)][2] = (held / total) as f32;
    }
    out
}

/// The share of the segment from `(ax, ay)` along `(vx, vy)` that lies in the
/// unit square about `(cx, cy)`: Liang and Barsky's clipping.
fn segment_in_square(ax: f64, ay: f64, vx: f64, vy: f64, cx: f64, cy: f64) -> f64 {
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [
        (-vx, ax - (cx - 0.5)),
        (vx, (cx + 0.5) - ax),
        (-vy, ay - (cy - 0.5)),
        (vy, (cy + 0.5) - ay),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return 0.0;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
        }
    }
    (t1 - t0).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings { enabled: true, ..Default::default() }
    }

    /// Nothing set: a single 1 at the origin, the image unchanged.
    #[test]
    fn nothing_set_is_the_identity() {
        let k = kernel((8, 6), (16, 16), &settings());
        assert_eq!(k[0], [1.0, 0.0, 1.0, 0.0]);
        let others: f32 = k.iter().skip(1).map(|v| v[0] + v[2]).sum();
        assert_eq!(others, 0.0);
    }

    /// Each part sums to its share, the smear to 1.
    #[test]
    fn the_parts_hold_their_shares() {
        let s = Settings { core: 2.0, halo_share: 0.2, halo_scale: 10.0, halo2_share: 0.1, halo2_scale: 40.0, smear: 3.3, smear_angle: 30.0, ..settings() };
        let k = kernel((64, 48), (128, 128), &s);
        let psf: f64 = k.iter().map(|v| v[0] as f64).sum();
        let smear: f64 = k.iter().map(|v| v[2] as f64).sum();
        assert!((psf - 1.0).abs() < 1e-5, "{psf}");
        assert!((smear - 1.0).abs() < 1e-6, "{smear}");
    }

    /// Along the rows, the smear is `hrsc.py`'s box with partial ends:
    /// `clip(L/2 + 0.5 - |x|, 0, 1)` normalised.
    #[test]
    fn a_smear_along_the_rows_is_hrscs_box() {
        let s = Settings { smear: 4.9, ..settings() };
        let k = kernel((32, 32), (64, 64), &s);
        let want = |x: f64| (4.9f64 / 2.0 + 0.5 - x.abs()).clamp(0.0, 1.0);
        let total: f64 = (-5..=5).map(|x| want(x as f64)).sum();
        for x in -5i64..=5 {
            let got = k[x.rem_euclid(64) as usize][2] as f64;
            assert!((got - want(x as f64) / total).abs() < 1e-6, "x {x}: {got}");
        }
        // Nothing off the row.
        assert_eq!(k[64][2], 0.0);
    }

    /// A segment through the middle of a square, wholly in it, and one past it.
    #[test]
    fn segments_clip_to_squares() {
        assert!((segment_in_square(-0.25, 0.0, 0.5, 0.0, 0.0, 0.0) - 1.0).abs() < 1e-12);
        assert!((segment_in_square(-1.0, 0.0, 2.0, 0.0, 0.0, 0.0) - 0.5).abs() < 1e-12);
        assert_eq!(segment_in_square(-1.0, 2.0, 2.0, 0.0, 0.0, 0.0), 0.0);
    }
}
