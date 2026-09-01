//! Band-integrated radiance from facet temperatures, on the GPU.
//!
//! Deliberately separable from `GpuTpm`. The model's two stages each run on
//! the CPU or the GPU independently, and all four combinations work:
//!
//! | TPM | radiance | temperatures |
//! |---|---|---|
//! | CPU | CPU | never leave numpy |
//! | GPU | CPU | one readback, `GpuTpm::surface` |
//! | CPU | GPU | one upload, `set_temperatures` |
//! | GPU | GPU | **nothing moves** -- `bind_tpm` reads the column buffer directly |
//!
//! The last row needs both built on one `Context`. Because `GpuTpm`
//! ping-pongs between two state buffers, `bind_tpm` builds a bind group for
//! each and `compute` picks by the TPM's current front, so no bind group is
//! created per call.

use crate::gpu::Context;
use std::sync::Arc;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    n_facets: u32,
    n_bands: u32,
    n_table: u32,
    temp_stride: u32,
    t_min: f32,
    t_inv_step: f32,
    stride_x: u32,
    _pad: u32,
}

pub struct GpuRadiance {
    ctx: Arc<Context>,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    tables: wgpu::Buffer,
    out: wgpu::Buffer,
    readback: wgpu::Buffer,
    /// Temperatures uploaded from the host, used when no TPM is bound.
    own_temps: wgpu::Buffer,
    own_params: wgpu::Buffer,
    own_bind: wgpu::BindGroup,
    /// One bind group per TPM state buffer, or none if none is bound.
    tpm_binds: Option<[wgpu::BindGroup; 2]>,
    n_facets: u32,
    n_bands: u32,
    n_table: u32,
    t_min: f32,
    t_inv_step: f32,
    stride_x: u32,
    dispatch: (u32, u32),
    /// Kept alive: the bind groups in `tpm_binds` reference it.
    tpm_params: Option<wgpu::Buffer>,
}

impl GpuRadiance {
    /// `tables` is `n_bands * n_table` of band-integrated radiance, laid out
    /// band-major, over temperatures `t_min .. t_max` inclusive.
    pub fn new(
        ctx: Arc<Context>,
        n_facets: u32,
        n_bands: u32,
        tables: &[f32],
        t_min: f32,
        t_max: f32,
    ) -> Result<Self, String> {
        if n_bands == 0 || tables.is_empty() || tables.len() % n_bands as usize != 0 {
            return Err("tables must be n_bands * n_table long".into());
        }
        let n_table = (tables.len() / n_bands as usize) as u32;
        if n_table < 2 {
            return Err("need at least 2 table entries".into());
        }
        if !(t_max > t_min) {
            return Err("t_max must exceed t_min".into());
        }
        let device = &ctx.device;
        let t_inv_step = (n_table - 1) as f32 / (t_max - t_min);
        let (gx, gy, stride_x) = Context::dispatch_2d((n_facets as u64) * (n_bands as u64));

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("radiance"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/radiance.wgsl").into()),
        });

        let mk_params = |temp_stride: u32| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("radiance params"),
                contents: bytemuck::bytes_of(&Params {
                    n_facets,
                    n_bands,
                    n_table,
                    temp_stride,
                    t_min,
                    t_inv_step,
                    stride_x,
                    _pad: 0,
                }),
                usage: wgpu::BufferUsages::UNIFORM,
            })
        };

        let tables_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("radiance tables"),
            contents: bytemuck::cast_slice(tables),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let n_out = (n_facets as u64) * (n_bands as u64);
        let out = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("radiance out"),
            size: n_out * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("radiance readback"),
            size: n_out * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let own_temps = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("radiance temps"),
            size: (n_facets as u64) * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

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
            label: Some("radiance"),
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
                entry(1, true),
                entry(2, true),
                entry(3, false),
            ],
        });

        let own_params = mk_params(1);
        let own_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("radiance own"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: own_params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: own_temps.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: tables_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: out.as_entire_binding(),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("radiance"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("radiance"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("band_radiance"),
            compilation_options: Default::default(),
            cache: None,
        });

        Ok(Self {
            ctx,
            pipeline,
            layout,
            tables: tables_buf,
            out,
            readback,
            own_temps,
            own_params,
            own_bind,
            tpm_binds: None,
            n_facets,
            n_bands,
            n_table,
            t_min,
            t_inv_step,
            stride_x,
            dispatch: (gx, gy),
            tpm_params: None,
        })
    }

    /// Read temperatures straight out of a `GpuTpm`'s column buffers.
    ///
    /// Zero copy, and the reason to build both on one `Context`. The TPM
    /// ping-pongs between two state buffers, so a bind group is made for each
    /// here and `compute` selects by the TPM's front -- nothing is created per
    /// call.
    ///
    /// The only difference from the uploaded path is `temp_stride`: a column
    /// buffer holds `n_nodes` values per facet and the surface is the first of
    /// each, so the shader strides rather than reading consecutively.
    pub fn bind_tpm(&mut self, tpm: &super::gpu::GpuTpm) -> Result<(), String> {
        if !Arc::ptr_eq(&self.ctx, tpm.context()) {
            return Err("the TPM was built on a different GPU context, so its \
                        buffers cannot be read here -- build both from one \
                        Context, or use set_temperatures"
                .into());
        }
        if tpm.n_facets() != self.n_facets {
            return Err(format!(
                "TPM has {} facets, radiance was built for {}",
                tpm.n_facets(),
                self.n_facets
            ));
        }
        let device = &self.ctx.device;
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("radiance params (tpm)"),
            contents: bytemuck::bytes_of(&Params {
                n_facets: self.n_facets,
                n_bands: self.n_bands,
                n_table: self.n_table,
                temp_stride: tpm.n_nodes(),
                t_min: self.t_min,
                t_inv_step: self.t_inv_step,
                stride_x: self.stride_x,
                _pad: 0,
            }),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let (a, b) = tpm.state_buffers();
        let mk = |temps: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("radiance tpm"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: temps.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.tables.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.out.as_entire_binding(),
                    },
                ],
            })
        };
        self.tpm_binds = Some([mk(a), mk(b)]);
        self.tpm_params = Some(params);
        Ok(())
    }

    /// Upload surface temperatures, one per facet. Use when the TPM ran on
    /// the CPU, or on another device.
    pub fn set_temperatures(&mut self, t: &[f32]) -> Result<(), String> {
        if t.len() != self.n_facets as usize {
            return Err(format!(
                "expected {} temperatures, got {}",
                self.n_facets,
                t.len()
            ));
        }
        self.tpm_binds = None;
        self.ctx
            .queue
            .write_buffer(&self.own_temps, 0, bytemuck::cast_slice(t));
        Ok(())
    }

    /// Radiance for every facet and band, `(n_facets, n_bands)` flattened.
    pub fn compute(&self, front: usize) -> Vec<f32> {
        let bind = match &self.tpm_binds {
            Some(b) => &b[front & 1],
            None => &self.own_bind,
        };
        let mut encoder = self
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("radiance"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("radiance"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, Some(bind), &[]);
            pass.dispatch_workgroups(self.dispatch.0, self.dispatch.1, 1);
        }
        self.ctx.queue.submit([encoder.finish()]);
        let n_out = (self.n_facets as u64) * (self.n_bands as u64);
        self.ctx.read_f32(&self.out, &self.readback, n_out)
    }

    pub fn n_facets(&self) -> u32 {
        self.n_facets
    }

    pub fn n_bands(&self) -> u32 {
        self.n_bands
    }
}
