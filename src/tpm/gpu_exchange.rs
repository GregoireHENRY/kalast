//! What facets send to one another, on the GPU: the twin of `exchange`.
//!
//! `exchange` says what is computed. `shaders/tpm_exchange.wgsl` is the same sums, one thread a facet over its own
//! row of the sparse view factors, a dispatch a reflection. The two differ in how fast they run and in rounding:
//! the GPU adds in single precision.
//!
//! A thread cannot say that the series has settled, and reading the radiosity back at each pass would cost more
//! than the passes. So the number of passes is decided here before anything is sent: each pass shrinks what is
//! left by at most the largest of `reflectivity x row sum` over the facets, which is known from the view factors
//! and the surfaces, and the passes are as many as bring that under the tolerance.

use std::num::NonZeroU64;
use std::sync::Arc;

use anyhow::{Result, bail};
use wgpu::util::DeviceExt;

use super::exchange::{Absorbed, Bounces, ViewFactors, check};
use crate::Float;
use crate::gpu::Context;
use crate::util::STEFAN_BOLTZMANN;

/// `Params` of `shaders/tpm_exchange.wgsl`, field for field.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    n_facets: u32,
    stride: u32,
    sigma: f32,
    pad: u32,
}

/// A whole buffer at a binding.
fn whole(binding: u32, b: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry { binding, resource: b.as_entire_binding() }
}

/// The exchange between the facets of one set of view factors and surfaces, on the GPU.
pub struct GpuExchange {
    ctx: Arc<Context>,
    factors: ViewFactors,
    albedo: Vec<Float>,
    emissivity: Vec<Float>,
    /// The largest `reflectivity x row sum`, in the visible and in the thermal infrared.
    ratio: (f64, f64),
    given: wgpu::Buffer,
    out: wgpu::Buffer,
    readback: wgpu::Buffer,
    /// `[0]` reads the first radiosity array and writes the second, `[1]` the other way.
    binds: [wgpu::BindGroup; 2],
    source: wgpu::ComputePipeline,
    bounce: wgpu::ComputePipeline,
    absorb: wgpu::ComputePipeline,
    dispatch: (u32, u32),
}

impl GpuExchange {
    /// For `factors`, a row and a column for each facet of every body, and each facet's albedo and emissivity.
    pub fn new(ctx: Arc<Context>, factors: &ViewFactors, albedo: &[Float], emissivity: &[Float]) -> Result<Self> {
        check(factors, albedo, emissivity, None, None, Bounces::default())?;
        let n = factors.rows();
        if n == 0 {
            bail!("an exchange between no facets");
        }
        let (start, column, value) = factors.csr();
        if value.len() > u32::MAX as usize {
            bail!("{} view factors: more than a row start of the GPU counts", value.len());
        }
        let sums = factors.row_sums();
        let most = |reflects: &dyn Fn(usize) -> f64| (0..n).fold(0.0_f64, |m, i| m.max(reflects(i) * sums[i] as f64));
        let ratio = (most(&|i| albedo[i] as f64), most(&|i| 1.0 - emissivity[i] as f64));

        let device = &ctx.device;
        let (gx, gy, stride) = Context::dispatch_2d(n as u64);
        let stored = |label: &str, contents: &[u8]| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            })
        };
        // a buffer cannot be empty: a matrix without a view factor keeps one that no row reaches
        let start32: Vec<u32> = start.iter().map(|&s| s as u32).collect();
        let column32: Vec<u32> = if column.is_empty() { vec![0] } else { column.to_vec() };
        let value32: Vec<f32> = if value.is_empty() { vec![0.0] } else { value.iter().map(|&v| v as f32).collect() };
        let props: Vec<f32> = (0..n).flat_map(|i| [albedo[i] as f32, emissivity[i] as f32]).collect();
        let start_buf = stored("tpm exchange start", bytemuck::cast_slice(&start32));
        let column_buf = stored("tpm exchange column", bytemuck::cast_slice(&column32));
        let value_buf = stored("tpm exchange value", bytemuck::cast_slice(&value32));
        let props_buf = stored("tpm exchange props", bytemuck::cast_slice(&props));
        let zeros = |label: &str, extra: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: 2 * n as u64 * 4,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | extra,
                mapped_at_creation: false,
            })
        };
        let given = zeros("tpm exchange given", wgpu::BufferUsages::empty());
        let radiosity = [zeros("tpm exchange radiosity a", wgpu::BufferUsages::empty()), zeros("tpm exchange radiosity b", wgpu::BufferUsages::empty())];
        let out = zeros("tpm exchange absorbed", wgpu::BufferUsages::COPY_SRC);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tpm exchange readback"),
            size: 2 * n as u64 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let params = Params { n_facets: n as u32, stride, sigma: STEFAN_BOLTZMANN as f32, pad: 0 };
        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("tpm exchange params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let storage = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tpm exchange"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(std::mem::size_of::<Params>() as u64),
                    },
                    count: None,
                },
                storage(1, true),
                storage(2, true),
                storage(3, true),
                storage(4, true),
                storage(5, true),
                storage(6, true),
                storage(7, false),
                storage(8, false),
            ],
        });
        let bind = |before: &wgpu::Buffer, after: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("tpm exchange"),
                layout: &layout,
                entries: &[
                    whole(0, &params_buf),
                    whole(1, &start_buf),
                    whole(2, &column_buf),
                    whole(3, &value_buf),
                    whole(4, &props_buf),
                    whole(5, &given),
                    whole(6, before),
                    whole(7, after),
                    whole(8, &out),
                ],
            })
        };
        let binds = [bind(&radiosity[0], &radiosity[1]), bind(&radiosity[1], &radiosity[0])];

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tpm exchange"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/tpm_exchange.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tpm exchange"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry_point: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Ok(Self {
            source: pipeline("source"),
            bounce: pipeline("bounce"),
            absorb: pipeline("absorb"),
            ctx,
            factors: factors.clone(),
            albedo: albedo.to_vec(),
            emissivity: emissivity.to_vec(),
            ratio,
            given,
            out,
            readback,
            binds,
            dispatch: (gx, gy),
        })
    }

    /// The facets.
    pub fn facets(&self) -> usize {
        self.factors.rows()
    }

    /// The passes `absorbed` will take for `bounces`: as given, or, to follow the reflections to the end, as many
    /// as bring what is left under the tolerance in both series.
    pub fn passes(&self, bounces: Bounces) -> Result<usize> {
        if !(bounces.tolerance > 0.0 && bounces.tolerance.is_finite()) {
            bail!("a tolerance of {} on the radiosity", bounces.tolerance);
        }
        if bounces.most != 0 {
            return Ok(bounces.most);
        }
        let needs = |ratio: f64, name: &str| -> Result<usize> {
            if ratio <= 0.0 {
                return Ok(1);
            }
            if ratio >= 1.0 {
                bail!("the {name} does not settle: a cavity that absorbs nothing");
            }
            // after k passes what is left is under ratio^k / (1 - ratio) of the largest source
            let k = ((bounces.tolerance as f64 * (1.0 - ratio)).ln() / ratio.ln()).ceil().max(0.0) as usize + 1;
            if k > Bounces::LIMIT {
                bail!("the {name} needs {k} reflections to settle to {}: more than {}", bounces.tolerance, Bounces::LIMIT);
            }
            Ok(k)
        };
        Ok(needs(self.ratio.0, "scattered sunlight")?.max(needs(self.ratio.1, "thermal radiation")?))
    }

    /// What every facet absorbs from the others, as `exchange::absorbed`. Blocking: the result is read back.
    pub fn absorbed(&self, incident: Option<&[Float]>, temperature: Option<&[Float]>, bounces: Bounces) -> Result<Absorbed> {
        check(&self.factors, &self.albedo, &self.emissivity, incident, temperature, bounces)?;
        let n = self.facets();
        let passes = self.passes(bounces)?;
        let given: Vec<f32> = (0..n).flat_map(|i| [incident.map_or(0.0, |s| s[i] as f32), temperature.map_or(0.0, |t| t[i] as f32)]).collect();
        self.ctx.queue.write_buffer(&self.given, 0, bytemuck::cast_slice(&given));

        let mut encoder = self.ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("tpm exchange") });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("tpm exchange"), timestamp_writes: None });
            // the sources go to the second array; each pass then reads the array the one before wrote
            let mut turn = 0;
            pass.set_pipeline(&self.source);
            pass.set_bind_group(0, Some(&self.binds[turn]), &[]);
            pass.dispatch_workgroups(self.dispatch.0, self.dispatch.1, 1);
            pass.set_pipeline(&self.bounce);
            for _ in 1..passes {
                turn = 1 - turn;
                pass.set_bind_group(0, Some(&self.binds[turn]), &[]);
                pass.dispatch_workgroups(self.dispatch.0, self.dispatch.1, 1);
            }
            turn = 1 - turn;
            pass.set_pipeline(&self.absorb);
            pass.set_bind_group(0, Some(&self.binds[turn]), &[]);
            pass.dispatch_workgroups(self.dispatch.0, self.dispatch.1, 1);
        }
        self.ctx.queue.submit([encoder.finish()]);
        let flat = self.ctx.read_f32(&self.out, &self.readback, 2 * n as u64);
        Ok(Absorbed {
            scattered: (0..n).map(|i| if incident.is_some() { flat[2 * i] as Float } else { 0.0 }).collect(),
            thermal: (0..n).map(|i| if temperature.is_some() { flat[2 * i + 1] as Float } else { 0.0 }).collect(),
            scattered_bounces: if incident.is_some() { passes } else { 0 },
            thermal_bounces: if temperature.is_some() { passes } else { 0 },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::exchange::absorbed;
    use super::*;
    use crate::Vec3;

    fn measured(what: &str) {
        if std::env::var("KALAST_MEASURE").is_ok() {
            println!("    measured: {what}");
        }
    }

    /// A device, or `None` with a word where there is none: these tests then check nothing.
    fn device() -> Option<Arc<Context>> {
        match Context::new() {
            Ok(ctx) => Some(ctx),
            Err(e) => {
                println!("    no GPU here, nothing checked: {e}");
                None
            }
        }
    }

    /// Numbers that look random and are the same every time.
    struct Dice(u64);
    impl Dice {
        fn roll(&mut self) -> Float {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 40) as f64 / (1u64 << 24) as f64) as Float
        }
    }

    /// `n` facets, each seeing about `per_row` others, every row summing to `sum`.
    fn scattered_factors(n: usize, per_row: usize, sum: Float, dice: &mut Dice) -> ViewFactors {
        let (mut start, mut column, mut value) = (vec![0], Vec::new(), Vec::new());
        for i in 0..n {
            let mut row: Vec<(u32, Float)> = Vec::new();
            for _ in 0..per_row {
                let j = (dice.roll() * n as Float) as usize % n;
                if j != i && !row.iter().any(|(c, _)| *c as usize == j) {
                    row.push((j as u32, 0.05 + dice.roll()));
                }
            }
            row.sort_by_key(|(c, _)| *c);
            let total: Float = row.iter().map(|(_, v)| v).sum();
            for (c, v) in row {
                column.push(c);
                value.push(v * sum / total);
            }
            start.push(value.len());
        }
        ViewFactors::from_csr(n, start, column, value).unwrap()
    }

    fn apart(a: &[Float], b: &[Float]) -> f64 {
        let scale = b.iter().fold(0.0_f64, |m, &v| m.max(v.abs() as f64)).max(1.0e-30);
        a.iter().zip(b).fold(0.0_f64, |m, (&x, &y)| m.max((x as f64 - y as f64).abs())) / scale
    }

    #[test]
    fn the_gpu_absorbs_what_the_cpu_absorbs() {
        let Some(ctx) = device() else { return };
        let mut dice = Dice(7);
        let n = 3000;
        let factors = scattered_factors(n, 40, 0.7, &mut dice);
        let albedo: Vec<Float> = (0..n).map(|_| 0.05 + 0.5 * dice.roll()).collect();
        let emissivity: Vec<Float> = (0..n).map(|_| 0.6 + 0.4 * dice.roll()).collect();
        let sun: Vec<Float> = (0..n).map(|_| (1361.0 * (dice.roll() - 0.4)).max(0.0)).collect();
        let temperature: Vec<Float> = (0..n).map(|_| 120.0 + 250.0 * dice.roll()).collect();
        let gpu = GpuExchange::new(ctx, &factors, &albedo, &emissivity).unwrap();
        for most in [1, 2, 5, 0] {
            let bounces = Bounces { most, ..Default::default() };
            let want = absorbed(&factors, &albedo, &emissivity, Some(&sun), Some(&temperature), bounces).unwrap();
            let got = gpu.absorbed(Some(&sun), Some(&temperature), bounces).unwrap();
            let (s, t) = (apart(&got.scattered, &want.scattered), apart(&got.thermal, &want.thermal));
            measured(&format!(
                "{n} facets, bounces {most}: the GPU within {s:.1e} (scattered) and {t:.1e} (thermal) of the CPU, in {} passes against {} and {}",
                got.scattered_bounces, want.scattered_bounces, want.thermal_bounces
            ));
            // followed to the end, the two stop at different passes, each within its tolerance of the sum
            let allowed = if most == 0 { 5.0e-6 } else { 2.0e-6 };
            assert!(s < allowed && t < allowed, "bounces {most}: {s} and {t}");
        }
        // one of the two left out is zero, and the other unchanged
        let only = gpu.absorbed(None, Some(&temperature), Bounces { most: 3, ..Default::default() }).unwrap();
        let both = gpu.absorbed(Some(&sun), Some(&temperature), Bounces { most: 3, ..Default::default() }).unwrap();
        assert!(only.scattered.iter().all(|&v| v == 0.0) && only.thermal == both.thermal && only.scattered_bounces == 0);
    }

    #[test]
    fn an_isothermal_closed_box_absorbs_what_it_emits_on_the_gpu() {
        let Some(ctx) = device() else { return };
        let (x, y, z) = (Vec3::X * 1.3, Vec3::Y * 0.8, Vec3::Z * 2.1);
        let o = Vec3::new(-0.4, 0.2, 0.1);
        let face = |origin: Vec3, e1: Vec3, e2: Vec3| [[origin, origin + e1, origin + e1 + e2], [origin, origin + e1 + e2, origin + e2]];
        let faces = [face(o, x, y), face(o + z, y, x), face(o, z, x), face(o + y, x, z), face(o, y, z), face(o + x, z, y)];
        let triangles: Vec<_> = faces.iter().flatten().copied().collect();
        let factors = ViewFactors::exact(&triangles);
        let temperature = vec![280.0; 12];
        let black = STEFAN_BOLTZMANN as f64 * 280.0_f64.powi(4);
        for e in [1.0, 0.9, 0.5, 0.2] {
            let gpu = GpuExchange::new(ctx.clone(), &factors, &[0.1; 12], &[e; 12]).unwrap();
            let got = gpu.absorbed(None, Some(&temperature), Bounces { most: 0, tolerance: 1.0e-7 }).unwrap();
            let worst = got.thermal.iter().fold(0.0_f64, |m, &v| m.max((v as f64 / (e as f64 * black) - 1.0).abs()));
            measured(&format!("isothermal box on the GPU, emissivity {e}: within {worst:.1e} of e sigma T^4 in {} passes", got.thermal_bounces));
            assert!(worst < 2.0e-5, "emissivity {e}: off by {worst}");
        }
    }

    #[test]
    fn the_gpu_refuses_what_the_cpu_refuses() {
        let Some(ctx) = device() else { return };
        let square = ViewFactors::from_csr(2, vec![0, 1, 2], vec![1, 0], vec![0.3, 0.1]).unwrap();
        assert!(GpuExchange::new(ctx.clone(), &square, &[0.1], &[0.9, 0.9]).is_err());
        assert!(GpuExchange::new(ctx.clone(), &square, &[0.1, 1.2], &[0.9, 0.9]).is_err());
        let wide = ViewFactors::from_csr(3, vec![0, 1, 2], vec![1, 2], vec![0.3, 0.1]).unwrap();
        assert!(GpuExchange::new(ctx.clone(), &wide, &[0.1, 0.1], &[0.9, 0.9]).is_err());
        let gpu = GpuExchange::new(ctx.clone(), &square, &[0.1, 0.1], &[0.9, 0.9]).unwrap();
        assert!(gpu.absorbed(Some(&[1.0]), None, Bounces::default()).is_err());
        assert!(gpu.absorbed(None, Some(&[200.0, -1.0]), Bounces::default()).is_err());
        assert!(gpu.absorbed(None, None, Bounces { most: 0, tolerance: 0.0 }).is_err());
        let none = gpu.absorbed(None, None, Bounces::default()).unwrap();
        assert!(none.scattered == vec![0.0, 0.0] && none.thermal == vec![0.0, 0.0]);
        // two mirrors facing each other and nothing else: it never settles, and says so before sending anything
        let mirrors = ViewFactors::from_csr(2, vec![0, 1, 2], vec![1, 0], vec![1.0, 1.0]).unwrap();
        let gpu = GpuExchange::new(ctx, &mirrors, &[1.0, 1.0], &[0.9, 0.9]).unwrap();
        let err = gpu.absorbed(Some(&[10.0, 0.0]), None, Bounces::default()).unwrap_err();
        assert!(err.to_string().contains("does not settle"), "{err}");
        // a number of bounces asked for is taken as given
        assert_eq!(gpu.passes(Bounces { most: 4, ..Default::default() }).unwrap(), 4);
    }
}
