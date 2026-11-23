//! The thermal columns stepped on the GPU by a scheme of one's choice: the
//! twin of `stepper::Stepper`, with what it takes -- one material or a
//! `Ground`, layered, graded, changing with temperature; a heat flow at the
//! base; the explicit scheme, RKC, backward Euler, Crank-Nicolson, BDF2.
//!
//! `stepper` says what the schemes are. `shaders/tpm_stepper.wgsl` is the
//! same arithmetic, one thread a facet, each going up and down its own
//! column. The two differ in how fast they run, and in rounding: float32 in
//! another order.
//!
//! The temperatures stay on the GPU between steps. They are given and read
//! back as `core::columns` lays them out, `(layers, facets)`.
//!
//! What the GPU does not do is refuse a step: a thread cannot report. A
//! step past what an explicit scheme reaches is refused here from the
//! ground's properties as given, before anything is sent; with laws that
//! change the ground with temperature the limit is the script's to keep
//! (`stability_maxdt_between`).

use std::num::NonZeroU64;
use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use ndarray::{Array2, ArrayView2};
use wgpu::util::DeviceExt;

use super::core::{GroundView, Laws, Newton, Thermal};
use super::stepper::{Rkc, Scheme};
use crate::Float;
use crate::gpu::Context;

/// `Params` of `shaders/tpm_stepper.wgsl`, field for field.
#[repr(C)]
#[derive(Copy, Clone, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    n_facets: u32,
    n_nodes: u32,
    stride: u32,
    node_stride: u32,
    facet_stride: u32,
    newton_max_iter: u32,
    capacity_law: u32,
    surface_given: u32,
    do_layers: u32,
    do_surface: u32,
    do_base: u32,
    first: u32,
    keep_before: u32,
    _pad0: [u32; 3],
    newton_threshold: f32,
    inv_dz2: f32,
    twodz: f32,
    dz: f32,
    chi: f32,
    c: [f32; 5],
    bottom: f32,
    gamma: f32,
    c_rhs: f32,
    w_before: f32,
    mu: f32,
    nu: f32,
    w_stage: f32,
    _pad1: [f32; 3],
}

/// `Sun` of `shaders/tpm_stepper_sun.wgsl`.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Sun {
    pos: [f32; 3],
    constant: f32,
    au: f32,
    n_facets: u32,
    stride: u32,
    facet_stride: u32,
}

/// What a ground gives the shader: three numbers a node, two a facet, its
/// laws, and the explicit scheme's largest step for it as given.
struct Uploaded {
    props: Vec<f32>,
    node_stride: u32,
    facet_props: Vec<f32>,
    facet_stride: u32,
    laws: Laws,
    euler_limit: Float,
}

impl Uploaded {
    fn of(thermal: Thermal<'_>, layers: usize, facets: usize, dz: Float) -> Result<Self> {
        let sigma = crate::util::STEFAN_BOLTZMANN;
        match thermal {
            Thermal::Body(prop) => {
                if !(prop.diffusivity > 0.0 && prop.conductivity > 0.0 && prop.density > 0.0 && prop.heat_capacity > 0.0) {
                    bail!(
                        "the diffusivity is {} and the conductivity {}: compute them from the thermal inertia first, \
                         with compute_conductivity_diffusivity()",
                        prop.diffusivity,
                        prop.conductivity
                    );
                }
                // The conductivity that gives the body's diffusivity with
                // its density and heat capacity: what the CPU steps by.
                let k = prop.diffusivity * prop.density * prop.heat_capacity;
                let mut props = Vec::with_capacity(3 * layers);
                // The surface balance reads the first node's conductivity.
                props.extend_from_slice(&[prop.conductivity as f32, prop.density as f32, prop.heat_capacity as f32]);
                for _ in 1..layers {
                    props.extend_from_slice(&[k as f32, prop.density as f32, prop.heat_capacity as f32]);
                }
                Ok(Self {
                    props,
                    node_stride: 0,
                    facet_props: vec![(sigma * prop.emissivity) as f32, (1.0 - prop.albedo) as f32],
                    facet_stride: 0,
                    laws: Laws::default(),
                    euler_limit: super::core::stability_maxdt(prop.diffusivity, dz * dz, 0.5),
                })
            }
            Thermal::Ground(g) => {
                g.check(layers, facets)?;
                let shared = g.conductivity.ncols() == 1 && g.density.ncols() == 1 && g.heat_capacity.ncols() == 1;
                let columns = if shared { 1 } else { facets };
                let at = |a: &ArrayView2<'_, Float>, i: usize, f: usize| {
                    let row = GroundView::layer(a, i);
                    if row.len() == 1 { row[0] } else { row[f] }
                };
                let mut props = Vec::with_capacity(3 * layers * columns);
                for f in 0..columns {
                    for i in 0..layers {
                        let (k, rho, c) = (at(&g.conductivity, i, f), at(&g.density, i, f), at(&g.heat_capacity, i, f));
                        if !(k >= 0.0 && rho > 0.0 && (c > 0.0 || g.laws.heat_capacity.is_some()) && k.is_finite()) {
                            bail!(
                                "layer {i} of facet {f} cannot be stepped: conductivity {k} W/m/K, density {rho} kg/m3, \
                                 heat capacity {c} J/kg/K"
                            );
                        }
                        props.extend_from_slice(&[k as f32, rho as f32, c as f32]);
                    }
                }
                let one = g.albedo.len() == 1 && g.emissivity.len() == 1;
                let pick = |a: &ndarray::ArrayView1<'_, Float>, f: usize| if a.len() == 1 { a[0] } else { a[f] };
                let mut facet_props = Vec::with_capacity(2 * if one { 1 } else { facets });
                for f in 0..if one { 1 } else { facets } {
                    facet_props.extend_from_slice(&[(sigma * pick(&g.emissivity, f)) as f32, (1.0 - pick(&g.albedo, f)) as f32]);
                }
                Ok(Self {
                    props,
                    node_stride: if shared { 0 } else { layers as u32 },
                    facet_props,
                    facet_stride: if one { 0 } else { 1 },
                    laws: g.laws,
                    euler_limit: g.stability_maxdt(layers, facets, dz, 0.5)?,
                })
            }
        }
    }
}

/// A whole buffer at a binding.
fn whole(binding: u32, b: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry { binding, resource: b.as_entire_binding() }
}

/// Steps the columns of one body on the GPU by a `Scheme`.
pub struct GpuStepper {
    ctx: Arc<Context>,
    scheme: Scheme,
    newton: Newton,
    layers: usize,
    facets: usize,
    dz: Float,
    bottom: Float,
    /// What every step's parameters start from.
    base: Params,
    /// One block of parameters a dispatch, a step's written at once.
    params_buf: wgpu::Buffer,
    block: u64,
    blocks: usize,
    implicit: wgpu::ComputePipeline,
    stage: wgpu::ComputePipeline,
    finish: wgpu::ComputePipeline,
    insolation: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    sun_layout: wgpu::BindGroupLayout,
    /// The three working arrays handed round: `binds[k]` is what the stage
    /// after `k` stages reads and writes.
    binds: [wgpu::BindGroup; 3],
    sun_bind: wgpu::BindGroup,
    t: wgpu::Buffer,
    rest: wgpu::Buffer,
    flux: wgpu::Buffer,
    work: [wgpu::Buffer; 3],
    props: wgpu::Buffer,
    facet_props: wgpu::Buffer,
    sun: wgpu::Buffer,
    positions: wgpu::Buffer,
    normals: wgpu::Buffer,
    lit: wgpu::Buffer,
    readback: wgpu::Buffer,
    have_geometry: bool,
    dispatch: (u32, u32),
    stride: u32,
    facet_stride: u32,
    euler_limit: Float,
    lawful: bool,
    /// The step before's length, once BDF2 has taken one.
    before: Option<Float>,
}

impl GpuStepper {
    /// The columns of a body of `facets` facets, `layers` deep and `dz` (m)
    /// apart, with the properties of `thermal`, to be stepped by `scheme`.
    /// The temperatures are whatever `upload` gives; nothing before that.
    pub fn new<'a>(
        ctx: Arc<Context>,
        layers: usize,
        facets: usize,
        dz: Float,
        thermal: impl Into<Thermal<'a>>,
        scheme: Scheme,
        newton: Newton,
    ) -> Result<Self> {
        if layers < 3 || facets == 0 {
            bail!("columns of {layers} layers under {facets} facets: 3 layers or more, a facet or more");
        }
        if !(dz > 0.0 && dz.is_finite()) {
            bail!("layers {dz} m apart: it must be positive");
        }
        if !(newton.threshold > 0.0 && newton.iterations >= 1) {
            bail!("Newton's method to {} K in {} iterations: a positive threshold, one iteration or more", newton.threshold, newton.iterations);
        }
        let stages = match scheme {
            Scheme::Rkc { stages } => {
                Rkc::new(stages)?;
                stages
            }
            _ => 1,
        };
        let up = Uploaded::of(thermal.into(), layers, facets, dz)?;
        let device = &ctx.device;
        let n = (layers * facets) as u64;
        let (gx, gy, stride) = Context::dispatch_2d(facets as u64);

        let zeros = |label: &str, len: u64, extra: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: len * 4,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | extra,
                mapped_at_creation: false,
            })
        };
        let t = zeros("tpm stepper t", n, wgpu::BufferUsages::COPY_SRC);
        let rest = zeros("tpm stepper rest", n, wgpu::BufferUsages::empty());
        let flux = zeros("tpm stepper flux", facets as u64, wgpu::BufferUsages::empty());
        let work = [
            zeros("tpm stepper work a", n, wgpu::BufferUsages::empty()),
            zeros("tpm stepper work b", n, wgpu::BufferUsages::empty()),
            zeros("tpm stepper work c", n, wgpu::BufferUsages::empty()),
        ];
        let positions = zeros("tpm stepper positions", 3 * facets as u64, wgpu::BufferUsages::empty());
        let normals = zeros("tpm stepper normals", 3 * facets as u64, wgpu::BufferUsages::empty());
        // Fully lit unless told otherwise.
        let lit = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("tpm stepper lit"),
            contents: bytemuck::cast_slice(&vec![1.0f32; facets]),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tpm stepper readback"),
            size: n * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let stored = |label: &str, contents: &[f32]| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytemuck::cast_slice(contents),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            })
        };
        let props = stored("tpm stepper props", &up.props);
        let facet_props = stored("tpm stepper facet props", &up.facet_props);

        // The parameters: a block a dispatch, at the offsets a device asks.
        let size = std::mem::size_of::<Params>() as u64;
        let align = device.limits().min_uniform_buffer_offset_alignment as u64;
        let block = size.div_ceil(align) * align;
        // The first call, the stages, the end.
        let blocks = stages + 2;
        let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tpm stepper params"),
            size: block * blocks as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sun = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tpm stepper sun"),
            size: std::mem::size_of::<Sun>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let storage = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let uniform = |binding: u32, dynamic: bool, size: u64| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: NonZeroU64::new(size),
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tpm stepper"),
            entries: &[
                uniform(0, true, size),
                storage(1, false),
                storage(2, false),
                storage(3, false),
                storage(4, true),
                storage(5, true),
                storage(6, false),
                storage(7, false),
                storage(8, false),
            ],
        });
        let sun_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tpm stepper sun"),
            entries: &[
                uniform(0, false, std::mem::size_of::<Sun>() as u64),
                storage(1, true),
                storage(2, true),
                storage(3, true),
                storage(4, true),
                storage(5, false),
            ],
        });
        let binds = Self::bind(device, &layout, &params_buf, size, &t, &rest, &flux, &props, &facet_props, &work);
        let sun_bind = Self::bind_sun(device, &sun_layout, &sun, &positions, &normals, &lit, &facet_props, &flux);

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tpm stepper"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/tpm_stepper.wgsl").into()),
        });
        let sun_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tpm stepper sun"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/tpm_stepper_sun.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tpm stepper"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let sun_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tpm stepper sun"),
            bind_group_layouts: &[Some(&sun_layout)],
            immediate_size: 0,
        });
        let pipeline = |layout: &wgpu::PipelineLayout, module: &wgpu::ShaderModule, entry_point: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry_point),
                layout: Some(layout),
                module,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let c = up.laws.heat_capacity.unwrap_or([0.0; 5]);
        let base = Params {
            n_facets: facets as u32,
            n_nodes: layers as u32,
            stride,
            node_stride: up.node_stride,
            facet_stride: up.facet_stride,
            newton_max_iter: newton.iterations.min(u32::MAX as usize) as u32,
            capacity_law: up.laws.heat_capacity.is_some() as u32,
            newton_threshold: newton.threshold as f32,
            inv_dz2: (1.0 / (dz * dz)) as f32,
            twodz: (2.0 * dz) as f32,
            dz: dz as f32,
            chi: up.laws.radiative as f32,
            c: c.map(|v| v as f32),
            ..Default::default()
        };
        Ok(Self {
            implicit: pipeline(&pipeline_layout, &module, "implicit"),
            stage: pipeline(&pipeline_layout, &module, "stage"),
            finish: pipeline(&pipeline_layout, &module, "finish"),
            insolation: pipeline(&sun_pipeline_layout, &sun_module, "insolation"),
            scheme,
            newton,
            layers,
            facets,
            dz,
            bottom: 0.0,
            base,
            params_buf,
            block,
            blocks,
            layout,
            sun_layout,
            binds,
            sun_bind,
            t,
            rest,
            flux,
            work,
            props,
            facet_props,
            sun,
            positions,
            normals,
            lit,
            readback,
            have_geometry: false,
            dispatch: (gx, gy),
            stride,
            facet_stride: up.facet_stride,
            euler_limit: up.euler_limit,
            lawful: !up.laws.is_constant(),
            before: None,
            ctx,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn bind(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        params: &wgpu::Buffer,
        size: u64,
        t: &wgpu::Buffer,
        rest: &wgpu::Buffer,
        flux: &wgpu::Buffer,
        props: &wgpu::Buffer,
        facet_props: &wgpu::Buffer,
        work: &[wgpu::Buffer; 3],
    ) -> [wgpu::BindGroup; 3] {
        // Each stage reads the two before it and writes the third: the
        // stage after reads what this one wrote as the one before it.
        let turn = |a: usize, b: usize, c: usize| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("tpm stepper"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: params, offset: 0, size: NonZeroU64::new(size) }),
                    },
                    whole(1, t),
                    whole(2, rest),
                    whole(3, flux),
                    whole(4, props),
                    whole(5, facet_props),
                    whole(6, &work[a]),
                    whole(7, &work[b]),
                    whole(8, &work[c]),
                ],
            })
        };
        [turn(0, 1, 2), turn(2, 0, 1), turn(1, 2, 0)]
    }

    #[allow(clippy::too_many_arguments)]
    fn bind_sun(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sun: &wgpu::Buffer,
        positions: &wgpu::Buffer,
        normals: &wgpu::Buffer,
        lit: &wgpu::Buffer,
        facet_props: &wgpu::Buffer,
        flux: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tpm stepper sun"),
            layout,
            entries: &[whole(0, sun), whole(1, positions), whole(2, normals), whole(3, lit), whole(4, facet_props), whole(5, flux)],
        })
    }

    pub fn scheme(&self) -> Scheme {
        self.scheme
    }

    pub fn dim(&self) -> (usize, usize) {
        (self.layers, self.facets)
    }

    /// The ground's properties sent again, after they were changed: the
    /// same columns, another `Properties` or `Ground` of their shape.
    pub fn set_properties<'a>(&mut self, thermal: impl Into<Thermal<'a>>) -> Result<()> {
        let up = Uploaded::of(thermal.into(), self.layers, self.facets, self.dz)?;
        let device = &self.ctx.device;
        let stored = |label: &str, contents: &[f32]| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytemuck::cast_slice(contents),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            })
        };
        self.props = stored("tpm stepper props", &up.props);
        self.facet_props = stored("tpm stepper facet props", &up.facet_props);
        let size = std::mem::size_of::<Params>() as u64;
        self.binds = Self::bind(device, &self.layout, &self.params_buf, size, &self.t, &self.rest, &self.flux, &self.props, &self.facet_props, &self.work);
        self.sun_bind = Self::bind_sun(device, &self.sun_layout, &self.sun, &self.positions, &self.normals, &self.lit, &self.facet_props, &self.flux);
        self.base.node_stride = up.node_stride;
        self.base.facet_stride = up.facet_stride;
        self.base.capacity_law = up.laws.heat_capacity.is_some() as u32;
        self.base.chi = up.laws.radiative as f32;
        self.base.c = up.laws.heat_capacity.unwrap_or([0.0; 5]).map(|v| v as f32);
        self.facet_stride = up.facet_stride;
        self.euler_limit = up.euler_limit;
        self.lawful = !up.laws.is_constant();
        Ok(())
    }

    /// The heat flow that enters the base of every column from below
    /// (W/m2): 0, the default, for none.
    pub fn set_bottom_flux(&mut self, q: Float) -> Result<()> {
        if !q.is_finite() {
            bail!("the heat flow at the bottom is {q} W/m2: it must be finite");
        }
        self.bottom = q;
        Ok(())
    }

    /// The temperatures, `(layers, facets)` as `core::columns` makes them.
    /// What the steps before left over is forgotten.
    pub fn upload(&mut self, t: ArrayView2<'_, Float>) -> Result<()> {
        if t.dim() != (self.layers, self.facets) {
            bail!("temperatures of shape {:?} for columns of {:?}, layers by facets", t.dim(), (self.layers, self.facets));
        }
        // A facet's column in one stretch.
        let mut flat = Vec::with_capacity(self.layers * self.facets);
        for f in 0..self.facets {
            flat.extend(t.column(f).iter().map(|&v| v as f32));
        }
        self.ctx.queue.write_buffer(&self.t, 0, bytemuck::cast_slice(&flat));
        self.reset();
        Ok(())
    }

    /// Forget the steps before: BDF2 starts again with a backward-Euler
    /// step, and nothing is carried over.
    pub fn reset(&mut self) {
        self.before = None;
        let zeros = vec![0.0f32; self.layers * self.facets];
        self.ctx.queue.write_buffer(&self.rest, 0, bytemuck::cast_slice(&zeros));
    }

    /// The temperatures read back, `(layers, facets)`. Blocking: for a
    /// result, not for every step.
    pub fn download(&self) -> Array2<Float> {
        let flat = self.ctx.read_f32(&self.t, &self.readback, (self.layers * self.facets) as u64);
        Array2::from_shape_fn((self.layers, self.facets), |(i, f)| flat[f * self.layers + i] as Float)
    }

    /// The surface temperatures read back, one a facet. Blocking.
    pub fn surface(&self) -> Vec<Float> {
        let flat = self.ctx.read_f32(&self.t, &self.readback, (self.layers * self.facets) as u64);
        (0..self.facets).map(|f| flat[f * self.layers] as Float).collect()
    }

    /// Facet centres and normals in the body's own frame, flat xyz, for
    /// `step_sun`. Once: they do not change.
    pub fn set_geometry(&mut self, positions: &[f32], normals: &[f32]) -> Result<()> {
        let want = 3 * self.facets;
        if positions.len() != want || normals.len() != want {
            bail!("positions and normals must be {want} long, 3 a facet: got {} and {}", positions.len(), normals.len());
        }
        self.ctx.queue.write_buffer(&self.positions, 0, bytemuck::cast_slice(positions));
        self.ctx.queue.write_buffer(&self.normals, 0, bytemuck::cast_slice(normals));
        self.have_geometry = true;
        Ok(())
    }

    /// The lit fraction of each facet, 1 where fully lit, for `step_sun`.
    pub fn set_lit(&mut self, lit: &[f32]) -> Result<()> {
        if lit.len() != self.facets {
            bail!("{} lit fractions for {} facets", lit.len(), self.facets);
        }
        self.ctx.queue.write_buffer(&self.lit, 0, bytemuck::cast_slice(lit));
        Ok(())
    }

    /// One step of `dt` (s), each surface from the flux it absorbs (W/m2).
    pub fn step(&mut self, flux: &[f32], dt: Float) -> Result<()> {
        if flux.len() != self.facets {
            bail!("{} absorbed fluxes for {} facets", flux.len(), self.facets);
        }
        if let Some((i, f)) = flux.iter().enumerate().find(|(_, f)| !(f.is_finite() && **f >= 0.0)) {
            bail!("the absorbed flux of facet {i} is {f} W/m2: it must be finite and not negative");
        }
        self.ready(dt)?;
        self.ctx.queue.write_buffer(&self.flux, 0, bytemuck::cast_slice(flux));
        self.advance(dt, false, false)
    }

    /// One step of `dt` (s), each surface's temperature given (K).
    pub fn step_surface(&mut self, temperature: &[f32], dt: Float) -> Result<()> {
        if temperature.len() != self.facets {
            bail!("{} surface temperatures for {} facets", temperature.len(), self.facets);
        }
        if let Some((i, v)) = temperature.iter().enumerate().find(|(_, v)| !(v.is_finite() && **v > 0.0)) {
            bail!("the surface temperature of facet {i} is {v} K: it must be finite and positive");
        }
        self.ready(dt)?;
        self.ctx.queue.write_buffer(&self.flux, 0, bytemuck::cast_slice(temperature));
        self.advance(dt, true, false)
    }

    /// One step of `dt` (s) with the sunlight each facet absorbs computed
    /// on the GPU: `S (1 - A) cos(i) lit / r^2`, from the Sun's position in
    /// the body's own frame (m). Nothing a facet crosses to the GPU.
    pub fn step_sun(&mut self, sun_pos: [f32; 3], au: f32, dt: Float) -> Result<()> {
        if !self.have_geometry {
            bail!("step_sun needs the facets' positions and normals: set_geometry first");
        }
        self.ready(dt)?;
        self.ctx.queue.write_buffer(
            &self.sun,
            0,
            bytemuck::bytes_of(&Sun {
                pos: sun_pos,
                constant: crate::util::SOLAR_CONSTANT as f32,
                au,
                n_facets: self.facets as u32,
                stride: self.stride,
                facet_stride: self.facet_stride,
            }),
        );
        self.advance(dt, false, true)
    }

    /// Whether a step of `dt` can be taken: refused here, since the GPU
    /// cannot refuse it.
    fn ready(&self, dt: Float) -> Result<()> {
        if !(dt > 0.0 && dt.is_finite()) {
            bail!("a step of {dt} s: it must be positive");
        }
        let reach = match self.scheme {
            Scheme::Explicit => 1.0,
            Scheme::Rkc { stages } => Rkc::new(stages)?.beta as Float / 2.0,
            _ => return Ok(()),
        };
        // A few ulps over the limit are the rounding of a step taken at it.
        if dt > reach * self.euler_limit * (1.0 + 16.0 * Float::EPSILON) {
            let need = match Scheme::rkc_stages(dt / self.euler_limit) {
                Ok(stages) if stages > 1 => format!("RKC takes it in {stages} stages"),
                _ => "an implicit scheme takes it".into(),
            };
            bail!(
                "unstable: a step of {dt} s, where the {} scheme reaches {} s for this ground{}; {need}",
                self.scheme.name(),
                reach * self.euler_limit,
                if self.lawful { " as given, less with its laws" } else { "" }
            );
        }
        Ok(())
    }

    /// The step's dispatches, with a block of parameters each.
    fn advance(&mut self, dt: Float, given: bool, insolate: bool) -> Result<()> {
        let base = Params { surface_given: given as u32, bottom: self.bottom as f32, ..self.base };
        // (pipeline, parameters, which turn of the working arrays)
        let mut calls: Vec<(&wgpu::ComputePipeline, Params, usize)> = Vec::new();
        match self.scheme {
            Scheme::BackwardEuler | Scheme::CrankNicolson | Scheme::Bdf2 => {
                let (mut gamma, mut c, mut w) = (dt, dt, 0.0 as Float);
                if self.scheme == Scheme::CrankNicolson {
                    gamma = 0.5 * dt;
                }
                if self.scheme == Scheme::Bdf2
                    && let Some(before) = self.before
                    && dt / before <= super::stepper::BDF2_LARGEST_RATIO
                {
                    let r = dt / before;
                    gamma = dt * (1.0 + r) / (1.0 + 2.0 * r);
                    c = gamma;
                    w = r * r / (1.0 + 2.0 * r);
                }
                let keep = (self.scheme == Scheme::Bdf2) as u32;
                calls.push((&self.implicit, Params { gamma: gamma as f32, c_rhs: c as f32, w_before: w as f32, keep_before: keep, ..base }, 0));
            }
            Scheme::Explicit => {
                // The surface from the layers as they stand, the layers,
                // then the base: `core`'s three calls.
                calls.push((&self.stage, Params { first: 1, do_surface: 1, ..base }, 0));
                calls.push((&self.stage, Params { do_layers: 1, do_base: 1, mu: 1.0, nu: 0.0, w_stage: dt as f32, ..base }, 1));
                calls.push((&self.finish, base, 2));
            }
            Scheme::Rkc { stages } => {
                let rkc = Rkc::new(stages)?;
                calls.push((&self.stage, Params { first: 1, do_surface: 1, do_base: 1, ..base }, 0));
                for j in 1..=stages {
                    let (mu, nu, tilde) = if j == 1 { (1.0, 0.0, rkc.first) } else { (rkc.mu[j], rkc.nu[j], rkc.tilde[j]) };
                    let p = Params { do_layers: 1, do_surface: 1, do_base: 1, mu: mu as f32, nu: nu as f32, w_stage: (tilde * dt as f64) as f32, ..base };
                    calls.push((&self.stage, p, j % 3));
                }
                calls.push((&self.finish, base, (stages + 1) % 3));
            }
        }
        if calls.len() > self.blocks {
            return Err(anyhow!("{} dispatches for {} blocks of parameters", calls.len(), self.blocks));
        }
        let mut bytes = vec![0u8; self.block as usize * calls.len()];
        for (k, (_, p, _)) in calls.iter().enumerate() {
            let at = k * self.block as usize;
            bytes[at..at + std::mem::size_of::<Params>()].copy_from_slice(bytemuck::bytes_of(p));
        }
        self.ctx.queue.write_buffer(&self.params_buf, 0, &bytes);

        let mut encoder = self.ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("tpm stepper") });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("tpm stepper step"), timestamp_writes: None });
            if insolate {
                pass.set_pipeline(&self.insolation);
                pass.set_bind_group(0, Some(&self.sun_bind), &[]);
                pass.dispatch_workgroups(self.dispatch.0, self.dispatch.1, 1);
            }
            for (k, (pipeline, _, turn)) in calls.iter().enumerate() {
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, Some(&self.binds[*turn]), &[(k as u64 * self.block) as u32]);
                pass.dispatch_workgroups(self.dispatch.0, self.dispatch.1, 1);
            }
        }
        self.ctx.queue.submit([encoder.finish()]);
        if self.scheme == Scheme::Bdf2 {
            self.before = Some(dt);
        }
        Ok(())
    }

    /// How far Newton's method takes the surface balance.
    pub fn newton(&self) -> Newton {
        self.newton
    }
}

#[cfg(test)]
mod tests {
    use ndarray::{Array1, Array2};

    use super::super::core::{Ground, columns, radiation_sun, stability_maxdt};
    use super::super::properties::{Properties, skin_depth_1};
    use super::super::stepper::{Stepper, Surface};
    use super::*;

    const PI: Float = std::f64::consts::PI as Float;
    const SCHEMES: [Scheme; 5] = [Scheme::Explicit, Scheme::Rkc { stages: 3 }, Scheme::BackwardEuler, Scheme::CrankNicolson, Scheme::Bdf2];

    fn measured(what: &str) {
        if std::env::var("KALAST_MEASURE").is_ok() {
            println!("    measured: {what}");
        }
    }

    /// A device, or `None` with a word where there is none: these tests
    /// then check nothing.
    fn device() -> Option<Arc<Context>> {
        match Context::new() {
            Ok(ctx) => Some(ctx),
            Err(e) => {
                println!("    no GPU here, nothing checked: {e}");
                None
            }
        }
    }

    fn material(ti: Float) -> Properties {
        let mut p = Properties {
            albedo: 0.1,
            emissivity: 0.9,
            density: 2000.0,
            heat_capacity: 600.0,
            thermal_inertia: ti,
            ..Default::default()
        };
        p.compute_conductivity_diffusivity();
        p
    }

    fn apart(a: &Array2<Float>, b: &Array2<Float>) -> Float {
        (a - b).mapv(Float::abs).fold(0.0, |m: Float, v| m.max(*v))
    }

    /// The step each scheme takes where the explicit one's largest is
    /// `limit`: under it, three times it in three stages, twelve times it.
    fn step_of(scheme: Scheme, limit: Float) -> Float {
        match scheme {
            Scheme::Explicit => 0.8 * limit,
            Scheme::Rkc { .. } => 3.0 * limit,
            _ => 12.0 * limit,
        }
    }

    /// `steps` steps of a day of sunlight on the CPU and on the GPU, from
    /// the same columns: the largest difference between the two at any of
    /// ten moments on the way, and the CPU's columns at the end.
    #[allow(clippy::too_many_arguments)]
    fn side_by_side<'a>(
        ctx: &Arc<Context>,
        scheme: Scheme,
        thermal: impl Into<Thermal<'a>>,
        start: &Array2<Float>,
        dz: Float,
        dt: Float,
        steps: usize,
        bottom: Float,
        peak: &Array1<Float>,
    ) -> (Float, Array2<Float>) {
        let thermal = thermal.into();
        let (layers, facets) = start.dim();
        let mut cpu = start.clone();
        let mut on_cpu = Stepper::new(scheme);
        let mut on_gpu = GpuStepper::new(ctx.clone(), layers, facets, dz, thermal, scheme, Newton::default()).unwrap();
        on_gpu.set_bottom_flux(bottom).unwrap();
        on_gpu.upload(start.view()).unwrap();
        let mut worst = 0.0 as Float;
        for k in 0..steps {
            let sun = (2.0 * PI * k as Float / steps as Float).cos().max(0.0);
            let flux = peak.mapv(|p| p * sun);
            on_cpu.step(cpu.view_mut(), Surface::Flux(flux.view()), thermal, dt, dz, bottom).unwrap();
            on_gpu.step(flux.as_slice().unwrap(), dt).unwrap();
            if (k + 1) % (steps / 10) == 0 {
                worst = worst.max(apart(&cpu, &on_gpu.download()));
            }
        }
        (worst, cpu)
    }

    fn tilted(layers: usize, facets: usize) -> Array2<Float> {
        let mut t = columns(layers, facets, 0.0);
        for ((z, f), v) in t.indexed_iter_mut() {
            *v = 230.0 + 9.0 * f as Float - 0.8 * z as Float;
        }
        t
    }

    /// One material: every scheme on the GPU is the CPU's, to what float32
    /// in another order leaves.
    #[test]
    fn every_scheme_is_the_cpus_on_one_material() {
        let Some(ctx) = device() else { return };
        let prop = material(200.0);
        let (layers, facets, dz) = (24, 5, 2e-3);
        let limit = stability_maxdt(prop.diffusivity, dz * dz, 0.5);
        let peak = ndarray::array![0.0, 150.0, 500.0, 900.0, 1250.0];
        for scheme in SCHEMES {
            let (e, cpu) = side_by_side(&ctx, scheme, &prop, &tilted(layers, facets), dz, step_of(scheme, limit), 400, 0.0, &peak);
            measured(&format!("one material, {}: the GPU within {e:.5} K of the CPU over 400 steps", scheme.name()));
            assert!(e < 5e-3, "{}: {e} K between the GPU and the CPU", scheme.name());
            assert!(cpu.row(0).iter().copied().fold(0.0, Float::max) > 300.0, "a day that warms the surface");
        }
    }

    /// A graded ground with a property of its own under each facet and in
    /// each layer, the Moon's laws, and a heat flow at the base.
    #[test]
    fn every_scheme_is_the_cpus_on_a_ground_with_laws() {
        let Some(ctx) = device() else { return };
        let prop = material(120.0);
        let (facets, dz) = (4, 1.5e-3);
        let (mut ground, widths) = Ground::graded(&prop, facets, dz, 0.4, 1.25).unwrap();
        let layers = widths.len();
        // A different ground under each facet, and a denser one at depth.
        let mut k = Array2::zeros((layers, facets));
        let mut rho = Array2::zeros((layers, facets));
        for ((i, f), v) in k.indexed_iter_mut() {
            *v = ground.conductivity[[i, 0]] * (1.0 + 0.3 * f as Float);
        }
        for ((i, f), v) in rho.indexed_iter_mut() {
            *v = ground.density[[i, 0]] * (1.0 + 0.02 * i as Float) * (1.0 - 0.05 * f as Float);
        }
        ground.conductivity = k;
        ground.density = rho;
        ground.emissivity = ndarray::array![0.95, 0.9, 0.85, 0.8];
        ground.laws = Laws::MOON;
        let limit = ground.stability_maxdt_between(dz, 0.5, 100.0, 420.0).unwrap();
        let peak = ndarray::array![100.0, 400.0, 800.0, 1200.0];
        for scheme in SCHEMES {
            let (e, cpu) = side_by_side(&ctx, scheme, &ground, &tilted(layers, facets), dz, step_of(scheme, limit), 400, 0.018, &peak);
            measured(&format!("a graded ground with the Moon's laws, {}: the GPU within {e:.5} K of the CPU ({} layers; the surface reached {:.1} K)", scheme.name(), layers, cpu.row(0).iter().copied().fold(0.0, Float::max)));
            assert!(e < 2e-2, "{}: {e} K between the GPU and the CPU", scheme.name());
        }

        // The same ground a layer at a time, one for every facet: the
        // properties sent as one column.
        let (mut shared, _) = Ground::graded(&prop, facets, dz, 0.4, 1.25).unwrap();
        shared.laws = Laws::MOON;
        let limit = shared.stability_maxdt_between(dz, 0.5, 100.0, 420.0).unwrap();
        let (e, _) = side_by_side(&ctx, Scheme::Bdf2, &shared, &tilted(layers, facets), dz, 12.0 * limit, 400, 0.0, &peak);
        measured(&format!("the same ground one a layer for every facet, bdf2: within {e:.5} K"));
        assert!(e < 2e-2, "{e} K");
    }

    /// A surface whose temperature is given, by every scheme.
    #[test]
    fn a_surface_given_is_the_cpus() {
        let Some(ctx) = device() else { return };
        let prop = material(200.0);
        let (layers, facets, dz) = (20, 3, 2e-3);
        let limit = stability_maxdt(prop.diffusivity, dz * dz, 0.5);
        for scheme in SCHEMES {
            let dt = step_of(scheme, limit);
            let mut cpu = tilted(layers, facets);
            let mut on_cpu = Stepper::new(scheme);
            let mut on_gpu = GpuStepper::new(ctx.clone(), layers, facets, dz, &prop, scheme, Newton::default()).unwrap();
            on_gpu.upload(cpu.view()).unwrap();
            for k in 0..200 {
                let top = Array1::from_shape_fn(facets, |f| 250.0 + 50.0 * (0.03 * k as Float + f as Float).sin());
                on_cpu.step(cpu.view_mut(), Surface::Temperature(top.view()), &prop, dt, dz, 0.3).unwrap();
                on_gpu.set_bottom_flux(0.3).unwrap();
                on_gpu.step_surface(top.as_slice().unwrap(), dt).unwrap();
            }
            let e = apart(&cpu, &on_gpu.download());
            measured(&format!("a surface given, {}: within {e:.5} K", scheme.name()));
            assert!(e < 5e-3, "{}: {e} K", scheme.name());
            assert_eq!(on_gpu.surface(), cpu.row(0).to_vec());
        }
    }

    /// A deep layer does not drift on the GPU: what a step cannot add to a
    /// temperature is kept there too. 6,400 steps of a wave, the base of the
    /// column against the CPU's.
    #[test]
    fn a_deep_layer_does_not_drift_on_the_gpu() {
        let Some(ctx) = device() else { return };
        let prop = material(320.0);
        let period: Float = 8136.0;
        let ls = skin_depth_1(prop.diffusivity, period);
        let dz = ls / 10.0;
        let layers = 81;
        let wave = |z: Float, time: Float| 300.0 + 100.0 * (-z / ls).exp() * (z / ls - 2.0 * PI * time / period).sin();
        let mut cpu = columns(layers, 1, 0.0);
        for i in 0..layers {
            cpu[[i, 0]] = wave(i as Float * dz, 0.0);
        }
        let dt = period / 1600.0;
        for scheme in [Scheme::CrankNicolson, Scheme::Rkc { stages: 2 }] {
            let mut cpu = cpu.clone();
            let mut on_cpu = Stepper::new(scheme);
            let mut on_gpu = GpuStepper::new(ctx.clone(), layers, 1, dz, &prop, scheme, Newton::default()).unwrap();
            on_gpu.upload(cpu.view()).unwrap();
            for k in 0..6400 {
                let top = [wave(0.0, (k + 1) as Float * dt)];
                on_cpu.step(cpu.view_mut(), Surface::Temperature(ndarray::arr1(&top).view()), &prop, dt, dz, 0.0).unwrap();
                on_gpu.step_surface(&top, dt).unwrap();
            }
            let gpu = on_gpu.download();
            let e = apart(&cpu, &gpu);
            // The wave is 0.03 K at the base, twice that against a base that
            // lets no heat through: the base is at the mean.
            let base = (gpu[[layers - 1, 0]] - 300.0).abs();
            measured(&format!("6,400 steps of a wave by {}: the GPU within {e:.5} K of the CPU, its base {base:.4} K from the mean", scheme.name()));
            assert!(e < 5e-3 && base < 0.1, "{}: {e} K from the CPU, the base {base} K from the mean", scheme.name());
        }
    }

    /// The sunlight computed on the GPU is the flux from the facets'
    /// geometry: facing the Sun, turned from it, in shadow, with each
    /// facet's albedo.
    #[test]
    fn the_sunlight_is_computed_on_the_gpu() {
        let Some(ctx) = device() else { return };
        let prop = material(200.0);
        let (layers, facets, dz) = (16, 4, 2e-3);
        let mut ground = Ground::new(&prop, layers, facets).unwrap();
        ground.albedo = ndarray::array![0.05, 0.1, 0.2, 0.3];
        let au = crate::util::AU as f32;
        let (dau, tilt): (Float, Float) = (1.3, 0.6);
        // Four facets at the origin: one facing the Sun, one tilted, one
        // turned away, one facing it in half shadow.
        let normals: [f32; 12] = [1.0, 0.0, 0.0, tilt.cos() as f32, tilt.sin() as f32, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        let cosi = ndarray::array![1.0, tilt.cos(), -1.0, 1.0];
        let lit = [1.0f32, 1.0, 1.0, 0.5];
        let dt = 8.0 * ground.stability_maxdt(dz, 0.5).unwrap();
        let mut cpu = columns(layers, facets, 250.0);
        let mut on_cpu = Stepper::new(Scheme::Bdf2);
        let mut on_gpu = GpuStepper::new(ctx.clone(), layers, facets, dz, &ground, Scheme::Bdf2, Newton::default()).unwrap();
        assert!(on_gpu.step_sun([dau as f32 * au, 0.0, 0.0], au, dt).unwrap_err().to_string().contains("set_geometry"));
        on_gpu.set_geometry(&[0.0; 12], &normals).unwrap();
        on_gpu.set_lit(&lit).unwrap();
        on_gpu.upload(cpu.view()).unwrap();
        let flux = Array1::from_shape_fn(facets, |f| radiation_sun(dau, cosi[f], ground.albedo[f]) * lit[f] as Float);
        for _ in 0..100 {
            on_cpu.step(cpu.view_mut(), Surface::Flux(flux.view()), &ground, dt, dz, 0.0).unwrap();
            on_gpu.step_sun([dau as f32 * au, 0.0, 0.0], au, dt).unwrap();
        }
        let e = apart(&cpu, &on_gpu.download());
        measured(&format!("the sunlight computed on the GPU: within {e:.5} K of the CPU given the same flux; surfaces at {:.1?} K", cpu.row(0).to_vec()));
        assert!(e < 5e-3, "{e} K");
        assert!(cpu[[0, 0]] > cpu[[0, 1]] && cpu[[0, 2]] < 200.0 && cpu[[0, 3]] < cpu[[0, 0]]);
    }

    /// What the GPU cannot refuse is refused before it is sent.
    #[test]
    fn a_gpu_step_refuses_what_cannot_be() {
        let Some(ctx) = device() else { return };
        let prop = material(200.0);
        let (layers, facets, dz) = (12, 2, 2e-3);
        let limit = stability_maxdt(prop.diffusivity, dz * dz, 0.5);
        let new = |scheme| GpuStepper::new(ctx.clone(), layers, facets, dz, &prop, scheme, Newton::default());
        let mut explicit = new(Scheme::Explicit).unwrap();
        explicit.upload(columns(layers, facets, 250.0).view()).unwrap();
        explicit.step(&[100.0, 200.0], 0.99 * limit).unwrap();
        let e = explicit.step(&[100.0, 200.0], 5.0 * limit).unwrap_err().to_string();
        assert!(e.contains("unstable") && e.contains("RKC takes it in 3 stages"), "{e}");
        let mut rkc = new(Scheme::Rkc { stages: 3 }).unwrap();
        rkc.upload(columns(layers, facets, 250.0).view()).unwrap();
        rkc.step(&[100.0, 200.0], 5.0 * limit).unwrap();
        assert!(rkc.step(&[100.0, 200.0], 50.0 * limit).is_err());
        let mut bdf2 = new(Scheme::Bdf2).unwrap();
        for (e, word) in [
            (bdf2.step(&[100.0], 50.0), "1 absorbed fluxes for 2 facets"),
            (bdf2.step(&[100.0, -1.0], 50.0), "facet 1"),
            (bdf2.step_surface(&[250.0, 0.0], 50.0), "facet 1"),
            (bdf2.step(&[100.0, 200.0], 0.0), "positive"),
            (bdf2.set_bottom_flux(Float::NAN), "finite"),
            (bdf2.upload(columns(layers, 3, 250.0).view()), "shape"),
            (bdf2.set_lit(&[1.0]), "1 lit fractions"),
        ] {
            let e = e.unwrap_err().to_string();
            assert!(e.contains(word), "{e}");
        }
        assert!(new(Scheme::Rkc { stages: 0 }).is_err());
        assert!(GpuStepper::new(ctx.clone(), 2, facets, dz, &prop, Scheme::Bdf2, Newton::default()).is_err());
        let raw = Properties { diffusivity: 0.0, ..prop };
        assert!(GpuStepper::new(ctx.clone(), layers, facets, dz, &raw, Scheme::Bdf2, Newton::default()).is_err());
    }
}
