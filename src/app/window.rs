use std::sync::Arc;

use glam::Mat4;

use crate::{Float, Vec3};

pub fn light_view_proj(
    pos: Vec3,
    target: Vec3,
    up: Vec3,
    side: Float,
    znear: Float,
    zfar: Float,
) -> Mat4 {
    let dir = (target - pos).normalize();
    let view = Mat4::look_to_rh(pos, dir, up);
    let proj = Mat4::orthographic_rh(-side, side, -side, side, znear, zfar);

    proj * view
}

pub struct Window {
    pub window: Arc<winit::window::Window>,
    pub instance: wgpu::Instance,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub is_surface_configured: bool,

    // 0: white cube
    // 1..: loaded by user in app.simulation.bodies
    pub meshes: Vec<super::gpu::MeshBuffer>,

    // Parallel to `meshes`: `Some` where a body supplied a lower-resolution
    // shadow stand-in, `None` to fall back to the entry in `meshes`. Same
    // indexing (element 0 is the light cube), so the shadow pass can pick
    // per body without a second lookup.
    pub shadow_meshes: Vec<Option<super::gpu::MeshBuffer>>,

    pub uniforms: super::uniform::Uniforms,
    pub passes: super::pass::Passes,

    pub export_frame: bool,
    pub frame_exporter: super::gpu::FrameExporter,

    // Built lazily: most runs never ask for a per-facet shadow query, and
    // compiling the compute pipeline is not free.
    pub facet_shadow: Option<super::facet_shadow::FacetShadowQuery>,

    // Same lazy treatment: the ID pass allocates two full-resolution
    // textures, which is wasted on any run that never asks for one.
    pub facet_id: Option<super::facet_id::FacetIdPass>,

    // Built on first use, like the other query passes: it allocates an atlas
    // and a weight texture that most runs never need.
    pub hemicube: Option<super::hemicube::Hemicube>,

    // Body model matrices as of the last `update`. The facet shadow query
    // needs the same transform the shadow map was built with, and it is
    // called from outside the borrow of `Simulation`.
    pub last_body_mats: Vec<Mat4>,
}

impl Window {
    pub async fn new(
        display: winit::event_loop::OwnedDisplayHandle,
        window: Arc<winit::window::Window>,
        config: &crate::app::config::Config,
        simulation: &crate::app::simulation::Simulation,
    ) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(display),
        ));
        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptionsBase {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .unwrap();

        // The adapted above isn't guaranteed to work on all devices.
        // In such case, use the adapter auto selection below.
        // let adapter = instance
        //     .enumerate_adapters(wgpu::Backends::all())
        //     .await.iter()
        //     .filter(|adapter| {
        //         adapter.is_surface_supported(&surface)
        //     })
        //     .next()
        //     .unwrap();

        let features_wgpu = wgpu::FeaturesWGPU::empty();
        // features_wgpu.insert(wgpu::FeaturesWGPU::POLYGON_MODE_LINE);

        let features_webgpu = wgpu::FeaturesWebGPU::empty();
        // features_webgpu.insert(wgpu::FeaturesWebGPU::DEPTH32FLOAT_STENCIL8);

        // Features::NON_FILL_POLYGON_MODE
        // Features::POLYGON_MODE_LINE
        // Features::POLYGON_MODE_POINT
        // Features::DEPTH_CLIP_CONTROL
        // Requires Features::CONSERVATIVE_RASTERIZATION

        // Default::default() for required_limits is wgpu::Limits::default(),
        // the conservative cross-backend-safe limits (e.g. 256 MiB max
        // buffer size) -- too small for full-resolution shape models
        // (unflattened Didymos alone needs a ~717 MB vertex buffer), and
        // needlessly so, since native backends (Metal here) typically
        // support far larger buffers. Request the adapter's actual limits
        // instead.
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: wgpu::Features {
                    features_wgpu,
                    features_webgpu,
                },
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await
            .unwrap();

        let caps = surface.get_capabilities(&adapter);

        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        if config.debug_window {
            println!("{:?}", format);
        }

        let size = window.inner_size();

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            format: format,
            width: size.width,
            height: size.height,
            present_mode: pick_present_mode(&caps, config.vsync),
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };

        // List of supported configurations by the adapter, device, surface.
        if config.debug_window {
            println!("[WINDOW] adapter features: {}", adapter.features());
            println!("[WINDOW] device features: {}", device.features());
            println!(
                "[WINDOW] surface capabilities present modes: {:?}",
                caps.present_modes
            );
        }

        let mut meshes = vec![];

        // TODO: ADD COLOR PER MESH?
        // InstanceInput is full in location (16)
        // need to move that to uniform or something else

        meshes.push(super::gpu::MeshBuffer::new(
            &device,
            &crate::meshes::cube::VERTICES,
            &crate::meshes::cube::INDICES,
            &super::gpu::InstanceInput::default(),
            false,
        ));

        // Element 0 pairs with the light cube, which the shadow pass skips.
        let mut shadow_meshes: Vec<Option<super::gpu::MeshBuffer>> = vec![None];

        let mut warned_wireframe = false;

        for body in &simulation.bodies {
            if let Some(mesh) = body.mesh.as_ref() {
                let mesh = mesh.borrow();

                // The wireframe recovers barycentrics from vertex_index,
                // which only holds for flat (non-indexed) meshes. Say so
                // once rather than silently dropping the overlay.
                if config.wireframe_mode != 0 && !mesh.is_flat() && !warned_wireframe {
                    warned_wireframe = true;
                    println!(
                        "[WINDOW] wireframe needs flat meshes (load with flatten=True); \
                         smooth meshes render shaded only"
                    );
                }

                if config.debug_window_mesh {
                    for v in &mesh.vertices {
                        println!("v: {}", v.pos);
                    }
                    println!("indices: {:?}", &mesh.indices);
                    println!("mat: {:?}", body.mat);
                }

                let instance = super::gpu::InstanceInput::new(body.mat);

                meshes.push(super::gpu::MeshBuffer::new(
                    &device,
                    &mesh.vertices,
                    &mesh.indices,
                    &instance,
                    mesh.is_flat(),
                ));

                shadow_meshes.push(body.shadow_mesh.as_ref().map(|shadow| {
                    let shadow = shadow.borrow();
                    super::gpu::MeshBuffer::new(
                        &device,
                        &shadow.vertices,
                        &shadow.indices,
                        &instance,
                        shadow.is_flat(),
                    )
                }));
            }
        }

        /*
        let texture = super::gpu::Texture::new_image_from_bytes(
            &device,
            &queue,
            include_bytes!("../../res/happy-tree.png"),
        );
        let textures = vec![texture];
        */

        let globals = super::gpu::UniformBuffer::new(&device, build_globals(config, None));

        let camera = super::uniform::Camera {
            view_proj: simulation
                .camera
                .view_proj(size.width as Float / size.height as Float)
                .unwrap(),
        };

        // Light needsmto be optimized in pos/proj znear/far/side
        // to have optimized shadow mapping resolution and reduce bias effects.

        let light = super::uniform::Light {
            view_proj: simulation
                .sun
                // .view_proj(size.width as Float / size.height as Float)
                .view_proj(1.0)
                .unwrap(),

            pos: simulation.sun.pos,
            color: super::gpu::color_vec3(&config.light_color),
            ..Default::default()
        };

        let view = super::gpu::UniformBuffer::new(&device, super::uniform::View { camera, light });

        let shadow = super::gpu::Texture::create_depth_texture_shadow_pass(
            &device,
            config.shadow_resolution,
            config.shadow_resolution,
        );

        let uniforms = super::uniform::Uniforms {
            globals,
            view,
            shadow,
        };

        let passes = super::pass::Passes::new(&device, surface_config.format, &config, &uniforms);

        Self {
            window,
            instance,
            surface,
            device,
            queue,
            surface_config,
            is_surface_configured: false,

            meshes,
            shadow_meshes,
            uniforms,
            passes,

            export_frame: false,
            frame_exporter: super::gpu::FrameExporter::new(
                config.export_dir.clone(),
                config.export_sync,
                config.export_max_queued as usize,
            ),

            facet_shadow: None,
            facet_id: None,
            hemicube: None,
            last_body_mats: vec![],
        }
    }

    /// Occluded fraction per facet for `body` (index into
    /// `simulation.bodies`), read back from the current shadow map.
    ///
    /// Reads the shadow map as it stands after the last rendered frame, so
    /// call it after at least one frame has been drawn for the epoch you
    /// care about. Blocking -- see `FacetShadowQuery::query`.
    ///
    /// Always queries the full-resolution render mesh, never the coarser
    /// `shadow_path` proxy: the proxy decides what the *map* contains, but
    /// the answer is wanted per real facet.
    pub fn facet_shadow_fractions(&mut self, body: usize) -> Vec<f32> {
        if self.facet_shadow.is_none() {
            self.facet_shadow = Some(super::facet_shadow::FacetShadowQuery::new(&self.device));
        }

        let Some(mesh) = self.meshes.get(1 + body) else {
            return vec![];
        };

        self.facet_shadow.as_ref().unwrap().query(
            &self.device,
            &self.queue,
            &self.uniforms.shadow,
            mesh,
            self.last_body_mats.get(body).copied().unwrap_or(Mat4::IDENTITY),
            self.uniforms.view.uniform.light.view_proj,
            self.uniforms.view.uniform.light.pos,
            &self.uniforms.globals.uniform,
        )
    }

    /// Facet index per pixel from the camera's point of view, plus the index
    /// offset applied to each body.
    ///
    /// Renders the scene again through the same view matrix into an integer
    /// target and reads it back. Costs a second geometry pass and a blocking
    /// readback, so it is meant for the frames a product is wanted from, not
    /// for every frame of a long run.
    pub fn facet_id_map(&mut self) -> (Vec<u32>, Vec<u32>, u32, u32) {
        let (w, h) = (self.surface_config.width, self.surface_config.height);
        if self.facet_id.is_none() {
            self.facet_id = Some(super::facet_id::FacetIdPass::new(
                &self.device,
                &self.uniforms.view.layout,
                w,
                h,
            ));
        }
        let pass = self.facet_id.as_mut().unwrap();
        pass.resize(&self.device, w, h);

        let camera_bind_group = self.uniforms.view.bind_group(&self.device);
        let (pixels, offsets) = pass.render_and_read(
            &self.device,
            &self.queue,
            &camera_bind_group,
            &self.meshes[1..],
        );
        (pixels, offsets, w, h)
    }

    /// View-factor rows for the given facets of `body`, by hemicube.
    ///
    /// Every loaded body is rendered into a shared index space, so one row
    /// carries the self view factors alongside the mutual ones. Returns
    /// `(rows, offsets, n_total)`; `rows` is row-major with entry
    /// `[i * n_total + j]` the fraction of energy leaving `facets[i]` that
    /// reaches global facet `j`, and `offsets[b]` is where body `b` starts.
    /// Occlusion is resolved by the depth test -- including by the *other*
    /// body, which is what makes a mutual eclipse block mutual heating.
    ///
    /// The CPU-side `mesh` is passed in rather than looked up: the window
    /// holds GPU buffers, and the facet centroids and normals the hemicubes
    /// are built from live on the simulation side. They are in that body's
    /// own frame, so the body's model matrix is applied here to place the
    /// hemicubes in the same world the meshes are drawn in.
    ///
    /// `near` matters more than it looks. It is tied to the smallest facet,
    /// because a near plane larger than a facet clips away that facet's
    /// immediate neighbours -- which are exactly the ones that dominate
    /// self-heating.
    pub fn hemicube_rows(
        &mut self,
        body: usize,
        mesh: &crate::mesh::Mesh,
        scene: Option<crate::mesh::Aabb>,
        facets: &[u32],
        resolution: u32,
        batch: u32,
    ) -> (Vec<f32>, Vec<u32>, u32) {
        if self.meshes.len() <= 1 + body {
            return (vec![], vec![], 0);
        }
        let model = self
            .last_body_mats
            .get(body)
            .copied()
            .unwrap_or(Mat4::IDENTITY);
        let normal_mat = crate::Mat3::from_mat4(model);

        let radius = mesh.bounds.radius().max(Float::EPSILON);
        let smallest = mesh
            .facets
            .iter()
            .map(|f| f.area)
            .fold(Float::INFINITY, Float::min)
            .max(Float::EPSILON)
            .sqrt();
        let near = (smallest * 1.0e-3).max(radius * 1.0e-7);

        // The far plane has to reach the whole scene, not just this body. A
        // companion sits at a distance set by the orbit, which for a small
        // secondary is many times its own radius: sizing `far` from the
        // requesting body alone put Didymos beyond Dimorphos's far plane at
        // the real 1.15 km separation, leaving a clipped remnant that read as
        // a mutual view factor 20x too small, and exactly zero past 1.5 km.
        // Measured against `(R/d)^2` over a separation sweep -- the falloff
        // now follows it instead of collapsing.
        //
        // Origins are spread over the body's surface rather than sitting at
        // its centre, so the reach is measured from the centre and the body's
        // own radius added back.
        let far = scene
            .map(|s| {
                let own = mesh.bounds.transform(&model);
                let c = own.center();
                let reach = s
                    .corners()
                    .iter()
                    .map(|p| (*p - c).length())
                    .fold(0.0 as Float, Float::max);
                (reach + own.radius()) * 1.01
            })
            .unwrap_or(0.0)
            .max(radius * 4.0);
        let proj = Mat4::perspective_rh(std::f64::consts::FRAC_PI_2 as Float, 1.0, near, far);

        let mut views = Vec::with_capacity(facets.len() * super::hemicube::FACES as usize);
        for &i in facets {
            let Some(facet) = mesh.facets.get(i as usize) else {
                continue;
            };
            let n = facet.normal.normalize_or_zero();
            if n.length_squared() < 0.5 {
                continue;
            }
            // Any tangent will do: the delta form factors are symmetric under
            // rotation about the normal, so the choice changes which side
            // face a given facet lands in, not the total.
            let helper = if n.x.abs() < 0.9 {
                crate::Vec3::X
            } else {
                crate::Vec3::Y
            };
            let t = n.cross(helper).normalize();
            let b = n.cross(t);

            // Lift off the surface so the facet does not fill its own
            // hemicube through depth fighting.
            let o = facet.pos + n * near * 2.0;
            let (o, n, t, b) = (
                model.transform_point3(o),
                (normal_mat * n).normalize_or_zero(),
                (normal_mat * t).normalize_or_zero(),
                (normal_mat * b).normalize_or_zero(),
            );
            for (dir, up) in [(n, t), (t, n), (-t, n), (b, n), (-b, n)] {
                views.push(proj * Mat4::look_to_rh(o, dir, up));
            }
        }

        if self.hemicube.is_none() {
            self.hemicube = Some(super::hemicube::Hemicube::new(
                &self.device,
                &self.queue,
                resolution,
                batch,
            ));
        }
        let hc = self.hemicube.as_ref().unwrap();
        hc.rows(&self.device, &self.queue, &self.meshes[1..], &views)
    }

    pub fn get_window(&self) -> &winit::window::Window {
        &self.window
    }

    pub fn configure_surface(&self) {
        // todo
    }

    pub fn center_cursor(&self) {
        let width = self.surface_config.width;
        let height = self.surface_config.height;
        let mid = (width / 2, height / 2);
        self.window
            .set_cursor_position(winit::dpi::PhysicalPosition::new(mid.0, mid.1))
            .unwrap();
    }

    pub fn reset_cursor(&self) {
        self.center_cursor();
        self.window.set_cursor_visible(true);
        self.window
            .set_cursor_grab(winit::window::CursorGrabMode::None)
            .unwrap();
    }

    pub fn toggle_export_frame(&mut self) {
        self.export_frame = !self.export_frame;
    }

    pub fn resize(&mut self, width: u32, height: u32, config: &crate::app::config::Config) {
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);

        self.passes
            .render
            .resize(&self.device, self.surface_config.format, width, height);
        self.passes.depth.resize(&self.device, width, height);

        let is_surface_configured = self.is_surface_configured;
        self.is_surface_configured = true;
        if !is_surface_configured && self.is_surface_configured {
            if config.debug_window {
                println!("[WINDOW] surface is now configured")
            }
        }
    }

    pub fn update(
        &mut self,
        simulation: &mut crate::app::simulation::Simulation,
        config: &crate::app::config::Config,
    ) {
        let width = self.surface_config.width;
        let height = self.surface_config.height;

        // Fit the frustums to wherever the bodies are now, then derive the
        // shadow constants from the light's fitted frustum. Both run every
        // frame because bodies and the sun move; user-pinned values survive
        // this untouched (see Projection::resolve_with).
        let shadow_fit = if let Some(bounds) = simulation.scene_bounds() {
            simulation.camera.fit_projection(&bounds, None);
            simulation
                .sun
                .fit_projection(&bounds, Some(config.shadow_resolution));

            Some(super::frame::fit_shadow(
                &simulation.sun.projection.resolved(),
                config.shadow_resolution,
            ))
        } else {
            simulation.camera.projection.resolve_manual();
            simulation.sun.projection.resolve_manual();
            None
        };

        // Globals used to be uploaded once at startup, which silently froze
        // every shading option after start(). The automatic shadow constants
        // change as the scene moves, so it now goes up every frame -- 80
        // bytes, and it makes the other options live as a side effect.
        self.uniforms.globals.uniform = build_globals(config, shadow_fit);
        self.queue.write_buffer(
            &self.uniforms.globals.buffer,
            0,
            bytemuck::bytes_of(&self.uniforms.globals.uniform),
        );

        self.uniforms.view.uniform.camera.view_proj = simulation
            .camera
            .view_proj(width as Float / height as Float)
            .unwrap();

        self.uniforms.view.uniform.light.view_proj = simulation
            .sun
            // .view_proj(size.width as Float / size.height as Float)
            .view_proj(1.0)
            .unwrap();

        self.uniforms.view.uniform.light.pos = simulation.sun.pos;

        self.queue.write_buffer(
            &self.uniforms.view.buffer,
            0,
            bytemuck::bytes_of(&self.uniforms.view.uniform),
        );

        self.last_body_mats.clear();
        self.last_body_mats
            .extend(simulation.bodies.iter().map(|b| b.mat));

        // skip light cube
        for ii in 0..simulation.bodies.len() {
            let flags = if self.meshes[1 + ii].is_flat {
                super::gpu::INSTANCE_FLAG_FLAT
            } else {
                0
            };

            let instance =
                super::gpu::InstanceInput::new_with_flags(simulation.bodies[ii].mat, flags);
            self.meshes[1 + ii].update_instance_buffer(&self.queue, &instance);

            // The shadow stand-in has to follow the same transform, or its
            // occluder would sit somewhere the visible body is not. Its own
            // flat flag applies, since it may be flattened differently.
            if let Some(shadow_buffer) = self.shadow_meshes[1 + ii].as_mut() {
                let shadow_flags = if shadow_buffer.is_flat {
                    super::gpu::INSTANCE_FLAG_FLAT
                } else {
                    0
                };
                let shadow_instance = super::gpu::InstanceInput::new_with_flags(
                    simulation.bodies[ii].mat,
                    shadow_flags,
                );
                shadow_buffer.update_instance_buffer(&self.queue, &shadow_instance);
            }

            let mesh = simulation.bodies[ii].mesh.as_ref().unwrap();
            if mesh.borrow().colors_dirty {
                self.meshes[1 + ii].update_attrib_buffer(&self.queue, &mesh.borrow().vertices);
                mesh.borrow_mut().colors_dirty = false;
            }
        }

        if simulation.export_once {
            self.export_frame = true;
        } else {
            self.export_frame = simulation.export;
        }
    }

    pub fn get_surface_texture(
        &mut self,
        config: &crate::app::config::Config,
    ) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => Some(texture),
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => None,
            wgpu::CurrentSurfaceTexture::Suboptimal(_) | wgpu::CurrentSurfaceTexture::Outdated => {
                if config.debug_window {
                    println!(
                        "[WINDOW] surface texture is suboptimal or outdated, need to reconfigure"
                    )
                }
                self.configure_surface();
                None
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                unreachable!("No error scope registered, so validation errors will panic")
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                if config.debug_window {
                    println!("[WINDOW] surface texture has been lost, need to recreate")
                }
                self.surface = self.instance.create_surface(self.window.clone()).unwrap();
                self.configure_surface();
                None
            }
        }
    }

    /// Draw a frame. `surface_texture` is `None` when there is nowhere to
    /// present -- an occluded window, most often.
    ///
    /// Everything the simulation depends on is offscreen: the shadow map the
    /// TPM reads, and the scene itself, which is drawn into `render_texture`
    /// and only blitted to the swapchain at the end. The surface view reaches
    /// nothing but the optional depth overlay. So a frame without a surface
    /// is a complete frame minus the blit and the present.
    ///
    /// This matters more than it sounds: macOS stops handing out drawables
    /// for an occluded window, and skipping the whole frame on that basis
    /// stopped the simulation dead -- a multi-hour run behind another window
    /// made no progress at all rather than merely rendering less.
    pub fn render(
        &mut self,
        surface_texture: Option<wgpu::SurfaceTexture>,
        config: &crate::app::config::Config,
    ) {
        let surface_view = surface_texture
            .as_ref()
            .map(|t| t.texture.create_view(&wgpu::TextureViewDescriptor::default()));
        let offscreen_view;
        let surface_view = match &surface_view {
            Some(v) => v,
            None => {
                offscreen_view = self
                    .passes
                    .render
                    .render_texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                &offscreen_view
            }
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        self.passes.render(
            &mut encoder,
            &surface_view,
            &self.uniforms.shadow,
            &self.meshes,
            &self.shadow_meshes,
            config,
        );

        if let Some(texture) = &surface_texture {
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.passes.render.render_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &texture.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: self.surface_config.width,
                    height: self.surface_config.height,
                    depth_or_array_layers: 1,
                },
            );
        }

        self.queue.submit([encoder.finish()]);

        // Non-blocking: drains any exports whose GPU->CPU copy finished
        // since the last frame. Runs every frame (not just when exporting)
        // so in-flight exports keep progressing even after export_frame
        // turns back off.
        self.frame_exporter.poll(&self.device);

        if self.export_frame {
            self.frame_exporter.export_frame(
                &self.device,
                &self.queue,
                &self.passes.render.render_texture,
                self.surface_config.width,
                self.surface_config.height,
            );
        }

        if let Some(texture) = surface_texture {
            texture.present();
        }
    }
}

/// Builds the globals uniform from config, filling in any shadow constant the
/// user left automatic from `shadow`.
///
/// `shadow` is `None` before the first fit (or when there is no geometry to
/// fit against), in which case an automatic parameter falls back to 0.0 --
/// i.e. no bias, which shows acne rather than silently hiding a failed fit.
fn build_globals(
    config: &crate::app::config::Config,
    shadow: Option<super::frame::ShadowFit>,
) -> super::uniform::Globals {
    super::uniform::Globals {
        color: super::gpu::color_vec3(&config.color),
        color_mode: config.color_mode,

        srgb_mode: config.srgb_mode,
        gamma: config.gamma,

        ambient_strength: config.ambient_strength,
        light_cube_scale: config.light_cube_scale,

        shadow_resolution: config.shadow_resolution,
        shadow_bias_scale: config
            .shadow_bias_scale
            .or(shadow.map(|s| s.bias_scale))
            .unwrap_or(0.0),
        shadow_bias_minimum: config
            .shadow_bias_minimum
            .or(shadow.map(|s| s.bias_minimum))
            .unwrap_or(0.0),
        shadow_normal_offset_scale: config
            .shadow_normal_offset_scale
            .or(shadow.map(|s| s.normal_offset_scale))
            .unwrap_or(0.0),
        shadow_pcf: config.shadow_pcf,

        extra: config.extra,

        wireframe_mode: config.wireframe_mode,
        wireframe_width: config.wireframe_width,
        wireframe_color: super::gpu::color_vec3(&config.wireframe_color),

        ..Default::default()
    }
}

/// Picks a present mode honouring `vsync`, falling back to the surface's
/// preferred mode (`present_modes[0]`, always supported) when the one we'd
/// want isn't available.
///
/// Note `present_modes[0]` is typically `Fifo` -- it was the unconditional
/// choice before this was configurable, which meant a GPU fast enough to
/// beat the display refresh rate was silently capped by it.
fn pick_present_mode(caps: &wgpu::SurfaceCapabilities, vsync: bool) -> wgpu::PresentMode {
    let wanted = if vsync {
        wgpu::PresentMode::Fifo
    } else {
        wgpu::PresentMode::Immediate
    };

    if caps.present_modes.contains(&wanted) {
        wanted
    } else {
        caps.present_modes[0]
    }
}
