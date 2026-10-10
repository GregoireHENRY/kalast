//! A reference image (`reference.enabled`): the image integrated, progressively,
//! over each pixel's area and the Sun's disc, to a stated error.
//!
//! Each frame draws one sample of every pixel -- the camera jittered within
//! the pixel (Halton's bases 2 and 3), and one ray to a point of the
//! limb-darkened disc (bases 5 and 7, `ray_reference` in `sun_rays.wgsl`) --
//! with the same shading as any frame, into a float target, and adds it to
//! one of four replicas' sums in turn. The window shows their mean. Each
//! replica's sequences are shifted by a fixed random amount per pixel
//! (Cranley-Patterson), so the four are independent estimates of the same
//! pixel and their spread is an honest standard error -- the naive one, from
//! one low-discrepancy sequence's samples, is far too pessimistic. The sum
//! stops once the 99.9th percentile of that error over the image is under
//! `reference.error`, or at `reference.max_samples`; anything that changes
//! the scene starts it again. No denoiser, no history, no clamp: a plain
//! mean of fixed sequences, the same image every run.
//!
//! See `notes/2026-10-10_ray_traced_shadows/`.

use wgpu::util::DeviceExt;

/// Independent estimates of each pixel, whose spread is its error.
pub const REPLICAS: u32 = 4;

/// The histogram's bins: zero, a third of an octave each from 2^-24, and
/// the largest error, as `cs_error` fills them.
const BINS: usize = 97;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    width: u32,
    height: u32,
    replica: u32,
    srgb_mode: u32,
    counts: [u32; 4],
}

pub struct Reference {
    pub size: (u32, u32),
    /// This frame's sample of every pixel, linear, for the main pass to draw
    /// into (`render::Pass::render_reference`).
    pub frame_view: wgpu::TextureView,
    _frame: wgpu::Texture,
    sums: wgpu::Buffer,
    uniform: wgpu::Buffer,
    histogram: wgpu::Buffer,
    histogram_read: wgpu::Buffer,
    group: wgpu::BindGroup,
    accumulate: wgpu::ComputePipeline,
    error_pipeline: wgpu::ComputePipeline,
    resolve: wgpu::RenderPipeline,
    /// Each replica's samples so far.
    pub counts: [u32; 4],
    /// What the scene was when the sum began (`Window::update`).
    pub scene: Option<Scene>,
    /// The 99.9th percentile of the pixels' standard error, last measured.
    pub error: Option<f32>,
    /// The sum has stopped: under `reference.error`, or at its last sample.
    pub done: bool,
}

impl Reference {
    pub fn new(device: &wgpu::Device, size: (u32, u32), target: wgpu::TextureFormat) -> Self {
        let (w, h) = (size.0.max(1), size.1.max(1));
        let frame = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("reference sample"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let frame_view = frame.create_view(&Default::default());
        let sums = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("reference sums"),
            size: REPLICAS as u64 * w as u64 * h as u64 * 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("reference"),
            contents: bytemuck::bytes_of(&Uniform { width: w, height: h, replica: 0, srgb_mode: 0, counts: [0; 4] }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let histogram = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("reference error histogram"),
            size: BINS as u64 * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let histogram_read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("reference error histogram, read"),
            size: BINS as u64 * 4,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let entry = |binding, visibility, ty| wgpu::BindGroupLayoutEntry { binding, visibility, ty, count: None };
        let storage = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("reference"),
            entries: &[
                entry(
                    0,
                    wgpu::ShaderStages::COMPUTE | wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(1, wgpu::ShaderStages::COMPUTE | wgpu::ShaderStages::FRAGMENT, storage),
                entry(
                    2,
                    wgpu::ShaderStages::COMPUTE,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(3, wgpu::ShaderStages::COMPUTE, storage),
            ],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("reference"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: sums.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&frame_view) },
                wgpu::BindGroupEntry { binding: 3, resource: histogram.as_entire_binding() },
            ],
        });
        let module = device.create_shader_module(crate::app::gpu::SHADER_REFERENCE);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("reference"),
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
        let accumulate = compute("cs_accumulate");
        let error_pipeline = compute("cs_error");
        let resolve = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("reference resolve"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_resolve"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_resolve"),
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
            sums,
            uniform,
            histogram,
            histogram_read,
            group,
            accumulate,
            error_pipeline,
            resolve,
            counts: [0; 4],
            scene: None,
            error: None,
            done: false,
        }
    }

    /// The samples summed so far, every replica's.
    pub fn samples(&self) -> u32 {
        self.counts.iter().sum()
    }

    /// The sum begun again, for the scene now.
    pub fn reset(&mut self, encoder: &mut wgpu::CommandEncoder, scene: Scene) {
        encoder.clear_buffer(&self.sums, 0, None);
        self.counts = [0; 4];
        self.scene = Some(scene);
        self.error = None;
        self.done = false;
    }

    /// Where within its pixel the next sample is taken, pixels from the
    /// centre, x right and y down: Halton's bases 2 and 3 for this replica's
    /// next index, shifted by the replica's own fixed amount.
    pub fn jitter(&self) -> (f32, f32) {
        let k = self.samples();
        let replica = (k % REPLICAS) as usize;
        let i = k / REPLICAS + 1;
        // Fixed, and far apart: each replica's own shift of the sequence.
        const SHIFT: [(f32, f32); 4] = [(0.0, 0.0), (0.5, 0.25), (0.25, 0.75), (0.75, 0.5)];
        let x = (radical_inverse(i, 2) + SHIFT[replica].0).fract();
        let y = (radical_inverse(i, 3) + SHIFT[replica].1).fract();
        (x - 0.5, y - 0.5)
    }

    /// This frame's sample, drawn into `frame_view`, added to its replica.
    pub fn accumulate(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, srgb_mode: u32) {
        let replica = self.samples() % REPLICAS;
        self.counts[replica as usize] += 1;
        // Written once for the frame, which both the sum and the mean
        // after it read: the counts as they are once this sample is in.
        queue.write_buffer(
            &self.uniform,
            0,
            bytemuck::bytes_of(&Uniform {
                width: self.size.0,
                height: self.size.1,
                replica,
                srgb_mode,
                counts: self.counts,
            }),
        );
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("reference sum"), timestamp_writes: None });
        pass.set_pipeline(&self.accumulate);
        pass.set_bind_group(0, Some(&self.group), &[]);
        pass.dispatch_workgroups(self.size.0.div_ceil(8), self.size.1.div_ceil(8), 1);
    }

    /// The mean into `target`, the image.
    pub fn resolve(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("reference mean"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.resolve);
        pass.set_bind_group(0, Some(&self.group), &[]);
        pass.draw(0..3, 0..1);
    }

    /// The 99.9th percentile of the pixels' standard error, and the largest:
    /// from the histogram `cs_error` fills, read back. Blocking.
    pub fn measure(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> (f32, f32) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("reference error") });
        encoder.clear_buffer(&self.histogram, 0, None);
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("reference error"), timestamp_writes: None });
            pass.set_pipeline(&self.error_pipeline);
            pass.set_bind_group(0, Some(&self.group), &[]);
            pass.dispatch_workgroups(self.size.0.div_ceil(8), self.size.1.div_ceil(8), 1);
        }
        encoder.copy_buffer_to_buffer(&self.histogram, 0, &self.histogram_read, 0, BINS as u64 * 4);
        queue.submit(Some(encoder.finish()));
        let slice = self.histogram_read.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
        let bins: Vec<u32> = bytemuck::cast_slice(&slice.get_mapped_range().unwrap()).to_vec();
        self.histogram_read.unmap();
        let largest = f32::from_bits(bins[BINS - 1]);
        (percentile(&bins[..BINS - 1], 0.999), largest)
    }

    /// Each pixel's mean, rows from the top, three channels, and its standard
    /// error, the largest of its channels': from the sums, read back.
    /// Blocking.
    pub fn read(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> (Vec<f32>, Vec<f32>) {
        let size = self.sums.size();
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("reference sums, read"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("reference read") });
        encoder.copy_buffer_to_buffer(&self.sums, 0, &read, 0, size);
        queue.submit(Some(encoder.finish()));
        let slice = read.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
        let data = slice.get_mapped_range().unwrap();
        let sums: &[f32] = bytemuck::cast_slice(&data);
        let (mean, error) = mean_and_error(sums, self.counts, self.size);
        drop(data);
        read.unmap();
        (mean, error)
    }
}

/// What a reference image is of (`Window::reference_scene`): the settings
/// and sizes exactly, the camera, the Sun and the bodies as numbers.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub settings: u64,
    pub size: (u32, u32),
    pub mesh_epoch: u64,
    pub values: Vec<f64>,
}

impl Scene {
    /// The same scene: the settings and sizes as they were, and every number
    /// within a millionth of what it was, relative -- the camera's basis is
    /// made orthonormal every frame, and its last place does not hold
    /// still, which no image can show.
    pub fn same(&self, other: &Scene) -> bool {
        self.settings == other.settings
            && self.size == other.size
            && self.mesh_epoch == other.mesh_epoch
            && self.values.len() == other.values.len()
            && self.values.iter().zip(&other.values).all(|(a, b)| {
                (a.is_nan() && b.is_nan()) || (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0)
            })
    }
}

/// The format a sample is drawn in: 32-bit float, so a mean of thousands is
/// not rounded to 8 bits a sample.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;

/// The radical inverse of `i` in `base`: Halton's sequence, as the shader's
/// `ray_radical_inverse`.
pub fn radical_inverse(mut i: u32, base: u32) -> f32 {
    let inv = 1.0 / base as f64;
    let (mut f, mut r) = (inv, 0.0f64);
    while i > 0 {
        r += (i % base) as f64 * f;
        i /= base;
        f *= inv;
    }
    r as f32
}

/// The value under which a share `q` of the counted errors lie: the upper
/// edge of the bin it falls in -- bin 0 is zero, bin `b` up to
/// `2^(b/3 - 24)`.
fn percentile(bins: &[u32], q: f64) -> f32 {
    let total: u64 = bins.iter().map(|&n| n as u64).sum();
    if total == 0 {
        return 0.0;
    }
    let want = (q * total as f64).ceil() as u64;
    let mut seen = 0u64;
    for (b, &n) in bins.iter().enumerate() {
        seen += n as u64;
        if seen >= want {
            return if b == 0 { 0.0 } else { 2f32.powf(b as f32 / 3.0 - 24.0) };
        }
    }
    2f32.powf(bins.len() as f32 / 3.0 - 24.0)
}

/// Per pixel: the replicas' means averaged, three channels; and the spread
/// of the replicas' means over the square root of their number, the largest
/// channel's, as `cs_error` has it.
fn mean_and_error(sums: &[f32], counts: [u32; 4], size: (u32, u32)) -> (Vec<f32>, Vec<f32>) {
    let n = (size.0 * size.1) as usize;
    let active: Vec<usize> = (0..4).filter(|&k| counts[k] > 0).collect();
    let mut mean = vec![0.0f32; n * 3];
    let mut error = vec![0.0f32; n];
    for p in 0..n {
        let mut m = [[0.0f32; 3]; 4];
        let mut avg = [0.0f32; 3];
        for &k in &active {
            for c in 0..3 {
                m[k][c] = sums[(k * n + p) * 4 + c] / counts[k] as f32;
                avg[c] += m[k][c] / active.len() as f32;
            }
        }
        mean[p * 3..p * 3 + 3].copy_from_slice(&avg);
        if active.len() > 1 {
            let r = active.len() as f32;
            let mut worst = 0.0f32;
            for c in 0..3 {
                let s2: f32 = active.iter().map(|&k| (m[k][c] - avg[c]).powi(2)).sum::<f32>() / (r - 1.0);
                worst = worst.max((s2 / r).sqrt());
            }
            error[p] = worst;
        }
    }
    (mean, error)
}

#[cfg(test)]
mod tests {
    /// Halton's first terms, base 2 and 3.
    #[test]
    fn the_radical_inverse_is_haltons() {
        let b2: Vec<f32> = (1..5).map(|i| super::radical_inverse(i, 2)).collect();
        let b3: Vec<f32> = (1..4).map(|i| super::radical_inverse(i, 3)).collect();
        assert_eq!(b2, vec![0.5, 0.25, 0.75, 0.125]);
        assert!((b3[0] - 1.0 / 3.0).abs() < 1e-7 && (b3[1] - 2.0 / 3.0).abs() < 1e-7 && (b3[2] - 1.0 / 9.0).abs() < 1e-7);
    }

    /// The percentile is the upper edge of the bin holding it.
    #[test]
    fn the_percentile_is_its_bins_upper_edge() {
        let mut bins = vec![0u32; 96];
        bins[0] = 990; // exactly zero
        bins[10] = 9;
        bins[40] = 1;
        assert_eq!(super::percentile(&bins, 0.99), 0.0);
        assert_eq!(super::percentile(&bins, 0.999), 2f32.powf(10.0 / 3.0 - 24.0));
        assert_eq!(super::percentile(&bins, 1.0), 2f32.powf(40.0 / 3.0 - 24.0));
    }

    /// Four replicas of one pixel: the mean of their means, and their spread
    /// over two.
    #[test]
    fn the_error_is_the_replicas_spread() {
        // One pixel; replica k summed (k + 1) over 2 samples.
        let sums: Vec<f32> = (0..4).flat_map(|k| [2.0 * (k as f32 + 1.0), 0.0, 0.0, 2.0]).collect();
        let (mean, error) = super::mean_and_error(&sums, [2; 4], (1, 1));
        assert!((mean[0] - 2.5).abs() < 1e-6);
        // Means 1, 2, 3, 4: sample sd sqrt(5/3), over sqrt(4).
        assert!((error[0] - (5.0f32 / 3.0).sqrt() / 2.0).abs() < 1e-6, "{}", error[0]);
    }
}
