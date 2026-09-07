//! The editor shell: a Blender-style layout with the renderer as one panel.
//!
//! Deliberately *not* an overlay on the render window. `App::start()` and
//! `App::step()` keep drawing the scene straight to the swapchain, which is
//! what a script run from a terminal wants and what every existing example
//! gets. The editor is a second entry point, `App::start_editor()`, and the
//! only thing it changes about the renderer is where the scene lands: into
//! `render_texture` sized to the viewport panel, which egui then samples.
//!
//! That texture already carries `TEXTURE_BINDING` -- the scene has always
//! been drawn offscreen and blitted at the end -- so showing it in a panel
//! costs a sampler, not a copy.

mod config_panel;

use std::collections::VecDeque;

/// Lines shown in the log panel.
///
/// Bounded rather than growing: a run that prints per frame would otherwise
/// hold every line it ever wrote for as long as the window is open.
pub struct Log {
    lines: VecDeque<String>,
    limit: usize,
}

impl Log {
    pub fn new(limit: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            limit,
        }
    }

    pub fn push(&mut self, line: impl Into<String>) {
        if self.lines.len() == self.limit {
            self.lines.pop_front();
        }
        self.lines.push_back(line.into());
    }

    pub fn lines(&self) -> impl Iterator<Item = &String> {
        self.lines.iter()
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }
}

pub struct Editor {
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,

    /// egui's handle on `render_texture`. Re-registered whenever that texture
    /// is reallocated, which a viewport resize does.
    viewport_texture: Option<egui::TextureId>,
    /// The size `render_texture` was registered at, to notice a reallocation.
    registered_size: (u32, u32),

    /// What the viewport panel measured last frame, in physical pixels.
    ///
    /// The scene has to be rendered *before* egui runs, so its size can only
    /// come from the previous frame's layout. On a resize the image is one
    /// frame stale, which is invisible; the alternative is a blank first
    /// frame at every new size.
    pub viewport_size: (u32, u32),

    /// The script buffer, so a simulation can be edited without leaving the
    /// window. Plain text, not a file handle: what is on screen is what
    /// `Run` executes, saved or not.
    pub script: String,
    pub script_path: String,
    /// Set when the buffer differs from what was last read or written.
    pub script_dirty: bool,
    /// Whether the buffer on screen is what is actually running.
    ///
    /// Cleared by an edit or an open, so Play re-runs after a change rather
    /// than resuming a scene built from text that is no longer on screen.
    pub script_ran: bool,
    /// Raised by the buttons, drained by the app, which owns the Python
    /// side. The UI cannot run anything itself -- it has no interpreter and
    /// no business holding the GIL mid-layout.
    pub run_request: bool,
    pub open_request: bool,
    pub save_request: bool,
}

impl Editor {
    pub fn new(
        window: &winit::window::Window,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> Self {
        let ctx = egui::Context::default();
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        let renderer = egui_wgpu::Renderer::new(
            device,
            format,
            egui_wgpu::RendererOptions {
                // The UI draws straight onto the swapchain, which has no
                // depth buffer and one sample. The scene's MSAA is the
                // scene's business -- it is resolved before egui sees it.
                depth_stencil_format: None,
                ..Default::default()
            },
        );
        Self {
            ctx,
            state,
            renderer,
            viewport_texture: None,
            registered_size: (0, 0),
            viewport_size: (
                window.inner_size().width.max(1),
                window.inner_size().height.max(1),
            ),
            script: String::new(),
            script_path: String::new(),
            script_dirty: false,
            script_ran: false,
            run_request: false,
            open_request: false,
            save_request: false,
        }
    }

    /// Draw the UI for one frame, onto `surface_view`.
    ///
    /// Called after the scene has been rendered into `render_texture`, so the
    /// viewport panel has something to show. Returns the size the viewport
    /// panel wants the *next* scene render to be, in physical pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        window: &winit::window::Window,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_view: &wgpu::TextureView,
        scene: &wgpu::Texture,
        scene_size: (u32, u32),
        config: &mut crate::app::config::Config,
        state: &mut crate::app::simulation::State,
        log: &mut Log,
        iteration_rate: f32,
    ) -> (u32, u32) {
        // Re-register only when the texture behind it is a different one. A
        // `TextureId` outlives a resize, but the view it points at does not.
        if self.viewport_texture.is_none() || self.registered_size != scene_size {
            let view = scene.create_view(&wgpu::TextureViewDescriptor::default());
            if let Some(id) = self.viewport_texture.take() {
                self.renderer.free_texture(&id);
            }
            self.viewport_texture =
                Some(self.renderer
                    .register_native_texture(device, &view, wgpu::FilterMode::Linear));
            self.registered_size = scene_size;
        }

        let raw = self.state.take_egui_input(window);
        let mut wanted = self.viewport_size;
        let texture_id = self.viewport_texture;
        let script = &mut self.script;
        let script_path = &mut self.script_path;
        let script_dirty = self.script_dirty;
        let script_ran = self.script_ran;
        let dirty = &mut self.script_dirty;
        let ran = &mut self.script_ran;
        let (mut run_request, mut open_request, mut save_request) = (false, false, false);

        let output = self.ctx.run_ui(raw, |ui_root| {
            let ppp = ui_root.ctx().pixels_per_point();

            egui::Panel::top("toolbar").show(ui_root, |ui| {
                ui.horizontal(|ui| {
                    // Play is the only way to start. A separate Run was the
                    // same button twice: both meant "go", and you had to press
                    // one then find the other.
                    //
                    // What it does depends on where the script stands. With
                    // nothing loaded there is nothing to play, so it is dead
                    // rather than advancing a clock nobody reads. With a
                    // script not yet run -- or edited since it last ran -- it
                    // runs it, which starts the simulation as a side effect.
                    // After that it is transport.
                    let have_script = !script.trim().is_empty();
                    let (label, hover): (&str, &str) = if !have_script {
                        ("\u{25b6} Play", "Open or write a script first")
                    } else if !script_ran {
                        ("\u{25b6} Play", "Run this script and start the simulation")
                    } else if state.is_paused {
                        ("\u{25b6} Play", "Resume")
                    } else {
                        ("\u{23f8} Pause", "Hold the simulation")
                    };
                    if ui
                        .add_enabled(have_script, egui::Button::new(label))
                        .on_hover_text(hover)
                        .clicked()
                    {
                        if script_ran {
                            state.is_paused = !state.is_paused;
                        } else {
                            // Running unpauses; see `serve_editor_requests`.
                            run_request = true;
                        }
                    }
                    if ui
                        .add_enabled(script_ran, egui::Button::new("\u{27f2} Restart"))
                        .on_hover_text("Clear the scene and run the script again")
                        .clicked()
                    {
                        run_request = true;
                    }
                    // One frame while paused: the same thing the render loop
                    // does, so the button cannot drift from the key.
                    if ui
                        .add_enabled(
                            script_ran && state.is_paused,
                            egui::Button::new("\u{23ed} Step"),
                        )
                        .on_hover_text("Advance one iteration")
                        .clicked()
                    {
                        state.is_paused = false;
                        state.pause_at = Some(state.iteration + 1);
                    }
                    ui.separator();
                    ui.label(format!("iteration {}", state.iteration));
                    let its = if state.is_paused { 0.0 } else { iteration_rate };
                    ui.label(format!("{its:.0} it/s"));
                    ui.weak(format!("{iteration_rate:.0} fps"));
                });
            });

            egui::Panel::bottom("log")
                .resizable(true)
                .default_size(160.0)
                .min_size(120.0)
                .show(ui_root, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Log").strong());
                        if ui.small_button("clear").clicked() {
                            log.clear();
                        }
                    });
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in log.lines() {
                                ui.label(egui::RichText::new(line).monospace());
                            }
                        });
                });

            egui::Panel::left("script")
                .resizable(true)
                .default_size(300.0)
                .min_size(180.0)
                .show(ui_root, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Script").strong());
                        if ui.small_button("open").clicked() {
                            open_request = true;
                        }
                        if ui.add_enabled(script_dirty, egui::Button::new("save").small()).clicked() {
                            save_request = true;
                        }
                    });
                    // Enter in the path field opens it: typing a filename and
                    // then having to find a button is a step nobody wants.
                    let path_edit = ui.add(
                        egui::TextEdit::singleline(script_path)
                            .hint_text("examples/crater_self_shadow/step.py")
                            .desired_width(f32::INFINITY),
                    );
                    if path_edit.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        open_request = true;
                    }
                    ui.separator();
                    // A layouter with no wrap width. Python read through a
                    // soft wrap is Python with its indentation destroyed, and
                    // `desired_width` alone does not prevent it: the default
                    // layouter wraps at whatever width it is handed.
                    let mut layouter =
                        |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _wrap: f32| {
                            let mut job = egui::text::LayoutJob::simple(
                                buf.as_str().to_owned(),
                                egui::FontId::monospace(12.0),
                                ui.visuals().text_color(),
                                f32::INFINITY,
                            );
                            job.wrap.max_width = f32::INFINITY;
                            ui.ctx().fonts_mut(|f| f.layout_job(job))
                        };
                    egui::ScrollArea::both().show(ui, |ui| {
                        let edit = ui.add(
                            egui::TextEdit::multiline(script)
                                .code_editor()
                                .layouter(&mut layouter)
                                .desired_width(f32::INFINITY)
                                .desired_rows(24),
                        );
                        if edit.changed() {
                            *dirty = true;
                            // What is running is no longer what is shown.
                            *ran = false;
                        }
                    });
                });

            egui::Panel::right("config")
                .resizable(true)
                .default_size(240.0)
                .min_size(180.0)
                .show(ui_root, |ui| {
                    ui.label(egui::RichText::new("Config").strong());
                    ui.separator();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        // Generated from `src/app/config.rs`, so a field
                        // added there gets a widget without anyone
                        // remembering to add one here.
                        config_panel::config_panel(ui, config);
                    });
                });

            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui_root, |ui| {
                    let avail = ui.available_size();
                    // What the *next* scene render should be, in physical
                    // pixels: egui works in points.
                    wanted = (
                        ((avail.x * ppp).round() as u32).max(1),
                        ((avail.y * ppp).round() as u32).max(1),
                    );
                    if let Some(id) = texture_id {
                        // Fit rather than fill: the scene was rendered at
                        // last frame's size, and stretching it to this
                        // frame's would distort during a drag.
                        let scene_aspect = scene_size.0 as f32 / scene_size.1.max(1) as f32;
                        let mut size = avail;
                        if size.x / size.y > scene_aspect {
                            size.x = size.y * scene_aspect;
                        } else {
                            size.y = size.x / scene_aspect;
                        }
                        ui.centered_and_justified(|ui| {
                            ui.add(egui::Image::new(egui::load::SizedTexture::new(id, size)));
                        });
                    }
                });
        });

        self.viewport_size = wanted;
        self.run_request |= run_request;
        self.open_request |= open_request;
        self.save_request |= save_request;
        self.state
            .handle_platform_output(window, output.platform_output);

        let jobs = self
            .ctx
            .tessellate(output.shapes, output.pixels_per_point);
        let desc = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [window.inner_size().width, window.inner_size().height],
            pixels_per_point: output.pixels_per_point,
        };

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("egui") });
        for (id, deltas) in &output.textures_delta.set {
            for delta in deltas {
                self.renderer.update_texture(device, queue, *id, delta);
            }
        }
        let extra = self
            .renderer
            .update_buffers(device, queue, &mut encoder, &jobs, &desc);

        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.renderer.render(&mut pass.forget_lifetime(), &jobs, &desc);
        }

        queue.submit(extra.into_iter().chain([encoder.finish()]));
        for id in &output.textures_delta.free {
            self.renderer.free_texture(id);
        }

        self.viewport_size
    }

    /// Give a window event to the UI first.
    ///
    /// Returns true when egui wants it -- a click on a slider, a keystroke in
    /// the script editor -- in which case the camera controller must not also
    /// act on it, or dragging a slider would orbit the scene behind it.
    pub fn on_window_event(
        &mut self,
        window: &winit::window::Window,
        event: &winit::event::WindowEvent,
    ) -> bool {
        self.state.on_window_event(window, event).consumed
    }
}
