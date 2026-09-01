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

use crate::gpu::Context;
use std::sync::Arc;
use wgpu::util::DeviceExt;

/// Rewritten each step. Everything else the insolation pass reads is static.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Sun {
    pos: [f32; 3],
    absorbed_at_1au: f32,
    au: f32,
    _pad: [f32; 3],
}

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

pub struct GpuTpm {
    ctx: Arc<Context>,
    surface_pipeline: wgpu::ComputePipeline,
    conduction_pipeline: wgpu::ComputePipeline,
    insolation_pipeline: wgpu::ComputePipeline,
    /// Two bind groups over the same buffers with `t_in`/`t_out` swapped, so
    /// a step is a dispatch and a flag flip rather than a copy.
    binds: [wgpu::BindGroup; 2],
    t: [wgpu::Buffer; 2],
    flux: wgpu::Buffer,
    sun: wgpu::Buffer,
    positions: wgpu::Buffer,
    normals: wgpu::Buffer,
    lit: wgpu::Buffer,
    readback: wgpu::Buffer,
    have_geometry: bool,
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
        ctx: Arc<Context>,
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

        let device = &ctx.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tpm"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/tpm.wgsl").into()),
        });

        let (sx, sy, stride_surface) = Context::dispatch_2d(n_facets as u64);
        let (cx, cy, stride_conduction) =
            Context::dispatch_2d((n_facets as u64) * (n_nodes as u64));
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
        let sun = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tpm sun"),
            size: std::mem::size_of::<Sun>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let vec3s = |label| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: (n_facets as u64) * 3 * 4,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let positions = vec3s("tpm positions");
        let normals = vec3s("tpm normals");
        // Fully lit unless told otherwise: a spin-up carries no shadowing.
        let lit = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("tpm lit"),
            contents: bytemuck::cast_slice(&vec![1.0f32; n_facets as usize]),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
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
                entry(3, false),
                entry(4, true),
                entry(5, true),
                entry(6, true),
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                entry(8, true),
                entry(9, true),
                entry(10, true),
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
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: sun.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 8,
                        resource: positions.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 9,
                        resource: normals.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 10,
                        resource: lit.as_entire_binding(),
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
            insolation_pipeline: pipeline("insolation"),
            binds,
            t,
            flux,
            sun,
            positions,
            normals,
            lit,
            readback,
            have_geometry: false,
            n_facets,
            n_nodes,
            dispatch_surface: (sx, sy),
            dispatch_conduction: (cx, cy),
            front: 0,
            ctx,
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
        self.ctx.queue
            .write_buffer(&self.t[self.front], 0, bytemuck::cast_slice(state));
        Ok(())
    }

    /// Upload facet centres and normals in the body frame, flat xyz.
    ///
    /// Static, so this is called once. It is what lets `step_sun` compute the
    /// boundary flux on the GPU: on the CPU the same pass re-streamed 151 MB
    /// of these every step at 3.1M facets -- 61 ms against the 8.7 ms the
    /// model itself costs -- to combine them with a sun direction that is
    /// three floats.
    pub fn set_geometry(&mut self, positions: &[f32], normals: &[f32]) -> Result<(), String> {
        let want = (self.n_facets as usize) * 3;
        if positions.len() != want || normals.len() != want {
            return Err(format!(
                "positions and normals must both be {want} long, got {} and {}",
                positions.len(),
                normals.len()
            ));
        }
        self.ctx
            .queue
            .write_buffer(&self.positions, 0, bytemuck::cast_slice(positions));
        self.ctx
            .queue
            .write_buffer(&self.normals, 0, bytemuck::cast_slice(normals));
        self.have_geometry = true;
        Ok(())
    }

    /// Lit fraction per facet, 1 where fully lit. Only needed with shadowing;
    /// it stays at 1 otherwise, which is what a spin-up wants.
    pub fn set_lit(&mut self, lit: &[f32]) -> Result<(), String> {
        if lit.len() != self.n_facets as usize {
            return Err(format!("lit must be {} long, got {}", self.n_facets, lit.len()));
        }
        self.ctx
            .queue
            .write_buffer(&self.lit, 0, bytemuck::cast_slice(lit));
        Ok(())
    }

    /// One timestep with the boundary flux computed on the GPU.
    ///
    /// `sun_pos` is the Sun in this body's frame, metres, and
    /// `absorbed_at_1au` the solar constant times `1 - albedo`.
    ///
    /// Needs `set_geometry` first. Nothing per-facet crosses the bus: a step
    /// uploads three floats.
    pub fn step_sun(
        &mut self,
        sun_pos: [f32; 3],
        absorbed_at_1au: f32,
        au: f32,
    ) -> Result<(), String> {
        if !self.have_geometry {
            return Err("call set_geometry before step_sun".into());
        }
        self.ctx.queue.write_buffer(
            &self.sun,
            0,
            bytemuck::bytes_of(&Sun {
                pos: sun_pos,
                absorbed_at_1au,
                au,
                _pad: [0.0; 3],
            }),
        );
        self.dispatch(true);
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
        self.ctx.queue
            .write_buffer(&self.flux, 0, bytemuck::cast_slice(flux));
        self.dispatch(false);
        Ok(())
    }

    /// Queue one step's passes. `insolate` prepends the flux computation, so
    /// the boundary source never comes from the host.
    fn dispatch(&mut self, insolate: bool) {
        let mut encoder = self
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("tpm") });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("tpm step"),
                timestamp_writes: None,
            });
            pass.set_bind_group(0, Some(&self.binds[self.front]), &[]);

            if insolate {
                pass.set_pipeline(&self.insolation_pipeline);
                pass.dispatch_workgroups(self.dispatch_surface.0, self.dispatch_surface.1, 1);
            }

            pass.set_pipeline(&self.surface_pipeline);
            pass.dispatch_workgroups(self.dispatch_surface.0, self.dispatch_surface.1, 1);

            pass.set_pipeline(&self.conduction_pipeline);
            pass.dispatch_workgroups(
                self.dispatch_conduction.0,
                self.dispatch_conduction.1,
                1,
            );
        }
        self.ctx.queue.submit([encoder.finish()]);
        self.front ^= 1;
    }

    /// Read the whole state back. Blocking, so call it for snapshots rather
    /// than every step -- the state stays resident otherwise.
    pub fn download(&self) -> Vec<f32> {
        let n_total = (self.n_facets as u64) * (self.n_nodes as u64);
        self.ctx
            .read_f32(&self.t[self.front], &self.readback, n_total)
    }

    /// Both state buffers, so another pass on the same device can read
    /// temperatures without a round trip through host memory. `front` says
    /// which is live.
    pub fn state_buffers(&self) -> (&wgpu::Buffer, &wgpu::Buffer) {
        (&self.t[0], &self.t[1])
    }

    pub fn front(&self) -> usize {
        self.front
    }

    pub fn context(&self) -> &Arc<Context> {
        &self.ctx
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
