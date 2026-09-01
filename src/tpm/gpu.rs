//! The thermophysical model on the GPU: one column per facet, no window.
//!
//! Measured on the CPU path at 10,000 facets and 34 nodes, `step_conduction`
//! is **93.5 %** of a spin-up's cost -- 1.13 ms of 1.21 ms a step. It is a
//! stencil over 340,000 independent nodes, which is what a GPU is for.
//!
//! The point is not the 10k mesh, where the CPU takes a tolerable 1.1 hours.
//! It is that the same spin-up costs an extrapolated **338 hours at 3.1M
//! facets**, which is not a run anyone starts. The state is only 0.43 GB in
//! f32, so the full-resolution shape model fits comfortably; it is time, not
//! memory, that rules it out on the CPU.
//!
//! Headless by construction: an adapter is requested with no surface, so a
//! spin-up needs no window. `kalast::gpu::compute` never worked -- its Python
//! shim imports `kalast._rs.gpu`, which does not exist -- so there was nothing
//! to build on.
//!
//! **f32.** WGSL has no f64 and the CPU model is float64, so the two cannot
//! agree exactly. The difference is measured rather than assumed; see
//! `examples/analytical/tpm_gpu_vs_cpu.py`.

use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    n_facets: u32,
    n_nodes: u32,
    se: f32,
    conductivity: f32,
    twodz: f32,
    newton_threshold: f32,
    newton_max_iter: u32,
    stride_surface: u32,
    stride_conduction: u32,
}

/// Split a linear invocation count into a dispatch that respects the 65,535
/// workgroups-per-dimension cap. Returns `(groups_x, groups_y, stride)`, with
/// `stride` the width in invocations so the shader can rebuild a linear index.
fn dispatch_2d(total: u64) -> (u32, u32, u32) {
    const WG: u64 = 64;
    const MAX: u64 = 65_535;
    let groups = total.div_ceil(WG).max(1);
    let gx = groups.min(MAX);
    let gy = groups.div_ceil(gx);
    (gx as u32, gy as u32, (gx * WG) as u32)
}

pub struct GpuTpm {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_pipeline: wgpu::ComputePipeline,
    conduction_pipeline: wgpu::ComputePipeline,
    /// Two bind groups over the same buffers with `t_in`/`t_out` swapped, so
    /// a step is a dispatch and a flag flip rather than a copy.
    binds: [wgpu::BindGroup; 2],
    t: [wgpu::Buffer; 2],
    flux: wgpu::Buffer,
    readback: wgpu::Buffer,
    n_facets: u32,
    n_nodes: u32,
    dispatch_surface: (u32, u32),
    dispatch_conduction: (u32, u32),
    /// Which of `t` currently holds the live state.
    front: usize,
}

impl GpuTpm {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        n_facets: u32,
        coef_lo: &[f32],
        coef_hi: &[f32],
        diffusivity: &[f32],
        se: f32,
        conductivity: f32,
        twodz: f32,
        newton_threshold: f32,
        newton_max_iter: u32,
    ) -> Result<Self, String> {
        let n_nodes = diffusivity.len() as u32;
        if n_nodes < 3 {
            return Err("need at least 3 nodes".into());
        }
        if coef_lo.len() != coef_hi.len() || coef_lo.len() + 2 != n_nodes as usize {
            return Err(format!(
                "coefficients must be n_nodes - 2 = {} long, got {} and {}",
                n_nodes - 2,
                coef_lo.len(),
                coef_hi.len()
            ));
        }

        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;

        // The adapter's real limits, not the conservative defaults: a 3.1M
        // facet column set is a 0.43 GB buffer, well past the 256 MiB the
        // cross-backend default allows.
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .map_err(|e| format!("no GPU device: {e}"))?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tpm"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/tpm.wgsl").into()),
        });

        let (sx, sy, stride_surface) = dispatch_2d(n_facets as u64);
        let (cx, cy, stride_conduction) =
            dispatch_2d((n_facets as u64) * (n_nodes as u64));
        let params = Params {
            n_facets,
            n_nodes,
            se,
            conductivity,
            twodz,
            newton_threshold,
            newton_max_iter,
            stride_surface,
            stride_conduction,
        };
        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("tpm params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let n_total = (n_facets as u64) * (n_nodes as u64);
        let state = |label| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: n_total * 4,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let t = [state("tpm state a"), state("tpm state b")];

        let flux = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tpm flux"),
            size: (n_facets as u64) * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tpm readback"),
            size: n_total * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let store = |contents: &[f32], label| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytemuck::cast_slice(contents),
                usage: wgpu::BufferUsages::STORAGE,
            })
        };
        let coef_lo_buf = store(coef_lo, "tpm coef_lo");
        let coef_hi_buf = store(coef_hi, "tpm coef_hi");
        let diff_buf = store(diffusivity, "tpm diffusivity");

        let entry = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tpm"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                entry(1, false),
                entry(2, false),
                entry(3, true),
                entry(4, true),
                entry(5, true),
                entry(6, true),
            ],
        });

        let make_bind = |a: &wgpu::Buffer, b: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("tpm"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: params_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: a.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: b.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: flux.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: coef_lo_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: coef_hi_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: diff_buf.as_entire_binding(),
                    },
                ],
            })
        };
        let binds = [make_bind(&t[0], &t[1]), make_bind(&t[1], &t[0])];

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tpm"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry_point| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };

        Ok(Self {
            surface_pipeline: pipeline("surface"),
            conduction_pipeline: pipeline("conduction"),
            binds,
            t,
            flux,
            readback,
            n_facets,
            n_nodes,
            dispatch_surface: (sx, sy),
            dispatch_conduction: (cx, cy),
            front: 0,
            device,
            queue,
        })
    }

    pub fn n_facets(&self) -> u32 {
        self.n_facets
    }

    pub fn n_nodes(&self) -> u32 {
        self.n_nodes
    }

    /// Upload the whole state, row-major `[facet][node]`.
    pub fn upload(&mut self, state: &[f32]) -> Result<(), String> {
        let want = (self.n_facets as usize) * (self.n_nodes as usize);
        if state.len() != want {
            return Err(format!("state must be {want} long, got {}", state.len()));
        }
        self.queue
            .write_buffer(&self.t[self.front], 0, bytemuck::cast_slice(state));
        Ok(())
    }

    /// One timestep: radiative surface, then conduction. `flux` is absorbed
    /// flux per facet, W/m2 -- everything the boundary sees, insolation and
    /// radiative heating together.
    pub fn step(&mut self, flux: &[f32]) -> Result<(), String> {
        if flux.len() != self.n_facets as usize {
            return Err(format!(
                "flux must be {} long, got {}",
                self.n_facets,
                flux.len()
            ));
        }
        self.queue
            .write_buffer(&self.flux, 0, bytemuck::cast_slice(flux));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("tpm") });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("tpm step"),
                timestamp_writes: None,
            });
            pass.set_bind_group(0, Some(&self.binds[self.front]), &[]);

            pass.set_pipeline(&self.surface_pipeline);
            pass.dispatch_workgroups(self.dispatch_surface.0, self.dispatch_surface.1, 1);

            pass.set_pipeline(&self.conduction_pipeline);
            pass.dispatch_workgroups(
                self.dispatch_conduction.0,
                self.dispatch_conduction.1,
                1,
            );
        }
        self.queue.submit([encoder.finish()]);
        self.front ^= 1;
        Ok(())
    }

    /// Read the whole state back. Blocking, so call it for snapshots rather
    /// than every step -- the state stays resident otherwise.
    pub fn download(&self) -> Vec<f32> {
        let n_total = (self.n_facets as u64) * (self.n_nodes as u64);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_buffer_to_buffer(&self.t[self.front], 0, &self.readback, 0, n_total * 4);
        self.queue.submit([encoder.finish()]);

        let slice = self.readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        let out = bytemuck::cast_slice::<u8, f32>(&slice.get_mapped_range()).to_vec();
        self.readback.unmap();
        out
    }

    /// Surface temperatures only, one per facet.
    pub fn surface(&self) -> Vec<f32> {
        let all = self.download();
        all.iter()
            .step_by(self.n_nodes as usize)
            .copied()
            .collect()
    }
}
