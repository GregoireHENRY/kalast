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
mod simulation_panel;

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

/// How close to an edge the pointer must come to summon a panel, in points.
const EDGE: f32 = 24.0;

/// How big each floating panel is when it has not been dragged: top, bottom,
/// left, right.
const FLOAT_DEFAULTS: [f32; 4] = [30.0, 160.0, 300.0, 240.0];

/// A *floating* panel smaller than this counts as put away rather than merely
/// narrow, and comes back at its usual size when next summoned.
///
/// The docked panels use a comfortable `min_size` instead -- 120 or 180 --
/// because there the same number is also the smallest a panel can be dragged,
/// and squeezing one through widths nothing can be read at is worse than
/// shutting it in one drag.
const COLLAPSE: f32 = 8.0;

/// Which panels to draw: `[top, bottom, left, right]`.
///
/// All of them unless the renderer has the window to itself, in which case
/// each is summoned by the pointer reaching its edge -- and stays while the
/// pointer is anywhere over it, since the 24-point strip is far narrower than
/// the panel and reaching for anything in one would otherwise dismiss it.
///
/// `panels` is where each was last drawn, or `Rect::NOTHING` for one that was
/// not; a panel that is not showing cannot keep itself showing.
///
/// A pure function so the arithmetic can be tested: driving a real pointer at
/// a real window is not possible from a test, because macOS delivers
/// mouse-moved events only to the front application.
fn reveal_panels(
    immersive: bool,
    pointer: Option<egui::Pos2>,
    screen: egui::Rect,
    panels: &[egui::Rect; 4],
) -> [bool; 4] {
    if !immersive {
        return [true; 4];
    }
    let Some(p) = pointer else {
        // The UI has never seen a pointer -- an unfocused window, usually.
        // Nothing to summon a panel with, so nothing is shown; the first move
        // inside the window fixes it.
        return [false; 4];
    };
    let near = [
        p.y <= screen.top() + EDGE,
        p.y >= screen.bottom() - EDGE,
        p.x <= screen.left() + EDGE,
        p.x >= screen.right() - EDGE,
    ];
    std::array::from_fn(|i| near[i] || panels[i].contains(p))
}

pub struct Editor {
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,

    /// egui's handle on `render_texture`. Re-registered whenever that texture
    /// is reallocated, which a viewport resize does.
    viewport_texture: Option<egui::TextureId>,
    /// The size `render_texture` was registered at.
    registered_size: (u32, u32),
    /// And which incarnation of it, since a rebuild makes a new texture at
    /// the same size -- see `Window::render_generation`.
    registered_generation: u64,

    /// What the viewport panel measured last frame, in physical pixels.
    ///
    /// The scene has to be rendered *before* egui runs, so its size can only
    /// come from the previous frame's layout. On a resize the image is one
    /// frame stale, which is invisible; the alternative is a blank first
    /// frame at every new size.
    pub viewport_size: (u32, u32),
    /// Whether each docked panel is open: top, bottom, left, right.
    ///
    /// egui flips these itself -- dragging a resize handle past the panel's
    /// minimum shuts it, and the thin handle it leaves behind at the edge
    /// drags it back, as does a double click. The state has to live somewhere
    /// that outlasts a frame, which is here.
    ///
    /// The toolbar is not resizable and is always open.
    docked_open: [bool; 4],

    /// How big each floating panel is: top, bottom, left, right.
    ///
    /// Kept here because a floating panel is an `Area`, which has none of a
    /// docked panel's resize machinery -- so it is dragged by a strip drawn
    /// on its inner edge and the size remembered between reveals.
    float_sizes: [f32; 4],
    /// Which floating panel is being dragged, if any.
    ///
    /// It has to stay revealed while it is: a drag wanders off the panel
    /// almost immediately, and losing the panel mid-drag would make it
    /// impossible to make one bigger.
    resizing: Option<usize>,

    /// Where each panel sits, in egui points, or `NOTHING` when it is not
    /// shown.
    ///
    /// Kept so a revealed panel stays revealed while the pointer is on it:
    /// the edge strip that summons the right panel is 24 points wide and the
    /// panel is 240, so "near the edge" stops being true the moment you
    /// reach for anything in it.
    panels: [egui::Rect; 4],

    /// Where the viewport panel sits, in egui points.
    ///
    /// The scene is an egui `Image`, so egui reports the pointer as its own
    /// whenever it is over one -- and the camera controller, which is given
    /// what egui does not want, never saw a drag on the scene. This says
    /// where "the scene" is so those events can be let through.
    pub viewport_rect: egui::Rect,

    /// The script buffer, so a simulation can be edited without leaving the
    /// window. Plain text, not a file handle: what is on screen is what
    /// `Run` executes, saved or not.
    pub script: String,
    pub script_path: String,
    /// Set when the buffer differs from what was last read or written.
    pub script_dirty: bool,
    /// Raised by the buttons, drained by the app, which owns the Python
    /// side. The UI cannot run anything itself -- it has no interpreter and
    /// no business holding the GIL mid-layout.
    pub run_request: bool,
    /// Set when the run was asked for by Restart rather than Play. The scene
    /// is rebuilt either way; this says whether it then runs.
    pub restart_request: bool,
    pub open_request: bool,
    pub save_request: bool,

    /// Which profile the Rust buttons act on. Release by default, because a
    /// debug build of this renderer is 2-15x slower and an example run for
    /// its numbers wants the fast one.
    pub rust_release: bool,
    pub build_request: bool,
    pub launch_request: bool,
    /// Play pressed on an example that was not built: the build is running
    /// and this says to launch it when it finishes.
    /// Whether a compiled binary exists for the `.rs` in the panel, at the
    /// selected profile. Refreshed by the app when the path, the profile or
    /// a build changes -- not every frame, since answering it means reading
    /// `Cargo.toml` and stat-ing a file.
    pub rust_built: bool,
    pub rust_key: (String, bool),
    pub was_building: bool,
    /// Held while a `cargo build` thread is running, so the buttons can go
    /// grey rather than starting a second one on top of the first.
    pub building: std::sync::Arc<std::sync::atomic::AtomicBool>,
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
            registered_generation: u64::MAX,
            panels: [egui::Rect::NOTHING; 4],
            docked_open: [true; 4],
            float_sizes: FLOAT_DEFAULTS,
            resizing: None,
            viewport_rect: egui::Rect::NOTHING,
            viewport_size: (
                window.inner_size().width.max(1),
                window.inner_size().height.max(1),
            ),
            script: String::new(),
            script_path: String::new(),
            script_dirty: false,
            run_request: false,
            restart_request: false,
            open_request: false,
            save_request: false,
            rust_release: true,
            build_request: false,
            launch_request: false,
            rust_built: false,
            rust_key: (String::new(), false),
            was_building: false,
            building: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
        // Size of `surface_view`, which is **not** always the window's. A
        // fullscreen toggle resizes the window at once, while the swapchain
        // follows a frame or two later on the `Resized` event. egui laying
        // out for the window and drawing into the surface meant a scissor
        // rect wider than the target -- "Scissor Rect { w: 3024 } is not
        // contained in the render target (1200, 800)" -- which wgpu treats as
        // fatal.
        surface_size: (u32, u32),
        scene: &wgpu::Texture,
        scene_size: (u32, u32),
        scene_generation: u64,
        config: &mut crate::app::config::Config,
        app_config: &mut crate::app::config::AppConfig,
        sim: &mut crate::app::simulation::Simulation,
        shared: &mut crate::app::Shared,
        iteration_rate: f32,
    ) -> (u32, u32) {
        // Re-register only when the texture behind it is a different one. A
        // `TextureId` outlives a resize, but the view it points at does not.
        if self.viewport_texture.is_none()
            || self.registered_size != scene_size
            || self.registered_generation != scene_generation
        {
            // A *non-sRGB* view of an sRGB texture: sampling returns the
            // stored bytes unchanged instead of converting them to linear.
            //
            // The scene is already encoded -- it is what an exported frame
            // contains, and what the plain window blits -- so egui must pass
            // it through, not decode it. With the default view the editor
            // showed every colour raised to the gamma: a flat 0.5 grey
            // measured 0.216 on screen, 0.5^2.2, against 0.502 in the plain
            // window and 0.502 in the exported PNG.
            let view = scene.create_view(&wgpu::TextureViewDescriptor {
                format: Some(scene.format().remove_srgb_suffix()),
                ..Default::default()
            });
            if let Some(id) = self.viewport_texture.take() {
                self.renderer.free_texture(&id);
            }
            self.viewport_texture =
                Some(self.renderer
                    .register_native_texture(device, &view, wgpu::FilterMode::Linear));
            self.registered_size = scene_size;
            self.registered_generation = scene_generation;
        }

        // Two panel closures both want the simulation -- the toolbar reads
        // the clock, the inspector shows everything else -- and both are
        // built before either runs. Only one is ever entered per frame, but
        // the borrow checker cannot see that, so the check moves to runtime.
        let sim = std::cell::RefCell::new(sim);

        let mut raw = self.state.take_egui_input(window);
        // Lay out for what is being drawn into, not for the window.
        let ppp = self.ctx.pixels_per_point();
        let screen = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(surface_size.0 as f32 / ppp, surface_size.1 as f32 / ppp),
        );
        raw.screen_rect = Some(screen);
        let mut wanted = self.viewport_size;
        let mut vp_rect = egui::Rect::NOTHING;

        // The scene takes the window and the panels get out of the way,
        // each coming back when the pointer reaches its edge -- and staying
        // while the pointer is on it, which the edge test alone would not
        // give.
        //
        // Driven by `focus` and not by `fullscreen`: one is about what is
        // inside the window, the other about the window itself.
        let immersive = app_config.focus;
        let pointer = self.ctx.pointer_latest_pos();
        let mut shows = reveal_panels(immersive, pointer, screen, &self.panels);
        // Whatever is being dragged stays up, wherever the pointer has got to.
        if let Some(i) = self.resizing {
            shows[i] = true;
        }
        let [show_top, show_bottom, show_left, show_right] = shows;
        let mut rects = [egui::Rect::NOTHING; 4];
        let mut out_sizes = self.float_sizes;
        let mut out_resizing = self.resizing;
        let was_shown = self.panels.map(|r| r.is_positive());
        let open_docked = self.docked_open;
        let mut out_open = self.docked_open;
        // Read before the panel closures are built: the toolbar needs to know
        // whether there is a script, the script panel needs the buffer, and
        // one cannot borrow it while the other holds it.
        let has_script = !self.script.trim().is_empty();
        shared.panels_shown = [show_top, show_bottom, show_left, show_right];
        shared.pointer = pointer.map(|p| (p.x, p.y));
        shared.ui_size = (screen.width(), screen.height());
        let texture_id = self.viewport_texture;
        let script = &mut self.script;
        let script_path = &mut self.script_path;
        let is_rust = script_path.trim_end().ends_with(".rs");
        let script_dirty = self.script_dirty;
        let script_ran = shared.script_ran;
        let drawn = shared.drawn_iteration;
        let log = &mut shared.log;
        let dirty = &mut self.script_dirty;
        let ran = &mut shared.script_ran;
        let (mut run_request, mut open_request, mut save_request) = (false, false, false);
        // A Rust example is built and launched rather than run in this
        // process, so the transport buttons do not apply to one.
        let building = self.building.load(std::sync::atomic::Ordering::SeqCst);
        let rust_built = self.rust_built;
        let native = shared.native;
        let rust_release = &mut self.rust_release;
        let (mut build_request, mut launch_request) = (false, false);
        let mut restart_request = false;

        let mut output = self.ctx.run_ui(raw, |ui_root| {
            let ppp = ui_root.ctx().pixels_per_point();

            // The scene itself, drawn the same way in both layouts and
            // differing only in what it is given.
            let scene_ui = |ui: &mut egui::Ui, into: egui::Rect| {
                if let Some(id) = texture_id {
                    // Fit rather than fill: the scene was rendered at last
                    // frame's size, and stretching it to this frame's would
                    // distort during a drag.
                    let scene_aspect = scene_size.0 as f32 / scene_size.1.max(1) as f32;
                    let mut size = into.size();
                    if size.x / size.y > scene_aspect {
                        size.x = size.y * scene_aspect;
                    } else {
                        size.y = size.x / scene_aspect;
                    }
                    // Painted into a rect worked out here, not laid out by
                    // the `Ui`. An `Area` is unbounded, so asking it to centre
                    // something centres it in an infinite region -- which put
                    // the scene in the bottom-right corner of the window,
                    // mostly out of view.
                    egui::Image::new(egui::load::SizedTexture::new(id, size))
                        .paint_at(ui, egui::Rect::from_center_size(into.center(), size));
                }
            };

            // Each panel's contents, named once so the same code can go in a
            // side panel or a floating one.
            let toolbar = app_config.toolbar.clone();
            let toolbar_ui = |ui: &mut egui::Ui| {
                let mut sim = sim.borrow_mut();
                let sim = &mut **sim;
                let diagnostics = &sim.diagnostics;
                let state = &mut sim.state;
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
                    // Native: this window *is* a compiled example, launched
                    // by an editor. There is no script to run -- the program
                    // is already running -- so Play is a pause toggle.
                    let have_script = if native {
                        true
                    } else if is_rust {
                        // Nothing to launch until it has been compiled.
                        rust_built
                    } else {
                        has_script
                    };
                    let (label, hover): (&str, &str) = if native {
                        if state.is_paused {
                            ("\u{25b6} Play", "Resume  (P)")
                        } else {
                            ("\u{23f8} Pause", "Hold the simulation  (P)")
                        }
                    } else if is_rust {
                        // Play is Play. For a Rust example that means
                        // launching the compiled binary in its own window --
                        // it is a separate program and cannot be hosted in
                        // this one.
                        if rust_built {
                            ("\u{25b6} Play", "Launch this example")
                        } else {
                            ("\u{25b6} Play", "Compile it first")
                        }
                    } else if !have_script {
                        ("\u{25b6} Play", "Open or write a script first")
                    } else if !script_ran {
                        ("\u{25b6} Play", "Run this script and start the simulation  (P)")
                    } else if state.is_paused {
                        ("\u{25b6} Play", "Resume  (P)")
                    } else {
                        ("\u{23f8} Pause", "Hold the simulation  (P)")
                    };
                    if ui
                        .add_enabled(have_script, egui::Button::new(label))
                        .on_hover_text(hover)
                        .clicked()
                    {
                        if native {
                            state.is_paused = !state.is_paused;
                        } else if is_rust {
                            launch_request = true;
                        } else if script_ran {
                            state.is_paused = !state.is_paused;
                        } else {
                            // Running unpauses; see `serve_editor_requests`.
                            run_request = true;
                        }
                    }
                    if ui
                        .add_enabled(script_ran && !is_rust && !native, egui::Button::new("\u{27f2} Restart"))
                        .on_hover_text(if native {
                            "This window is the example; close it and launch again"
                        } else if is_rust {
                            "A Rust example builds its own scene, in its own process"
                        } else {
                            "Rebuild the scene from the script and stop at the start"
                        })
                        .clicked()
                    {
                        run_request = true;
                        restart_request = true;
                    }
                    // One frame while paused: the same thing the render loop
                    // does, so the button cannot drift from the key.
                    if ui
                        .add_enabled(
                            // A hosted script must have run; a native window
                            // is itself the run. A `.rs` in the panel has
                            // nothing to step here at all until it is
                            // compiled and launched into its own window.
                            (script_ran || native) && state.is_paused,
                            egui::Button::new("\u{23ed} Step"),
                        )
                        .on_hover_text("Advance one iteration  (K)")
                        .clicked()
                    {
                        state.is_paused = false;
                        state.pause_at = Some(state.iteration + 1);
                    }
                    ui.separator();
                    // The same template a HUD takes, so the toolbar says
                    // whatever this run wants it to -- and `{drawn}` rather
                    // than `{it}` by default, because once the frame for
                    // iteration 0 is drawn `state.iteration` is already 1,
                    // and "1" under a picture of 0 is a lie of one frame.
                    if !toolbar.is_empty() {
                        ui.label(crate::app::expand_hud(
                            &toolbar,
                            state,
                            iteration_rate as crate::Float,
                            diagnostics,
                            drawn,
                        ))
                        .on_hover_text(
                            "app.config.toolbar -- {drawn} {it} {its} {fps} {ms} {bodies} {paused} {warn}",
                        );
                    }
                });
            };
            let log_ui = |ui: &mut egui::Ui| {
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
                };
            let script_ui = |ui: &mut egui::Ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(if native { "Running" } else { "Script" }).strong());
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
                    // A Rust example is a separate program: it links kalast
                    // as a library and opens its own window, so it cannot be
                    // hosted in this one the way a script is. Build it and
                    // launch it instead -- cargo and the example both inherit
                    // this process's redirected stdout, so their output still
                    // arrives in the Log below.
                    // Not in a launched example: it is already running the
                    // thing, and there is no Play left to launch a rebuild
                    // with -- Play is its pause button. The source is here to
                    // read, not to act on.
                    if is_rust && !native {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Rust").weak());
                            ui.selectable_value(rust_release, false, "debug")
                                .on_hover_text("cargo build --example ...");
                            ui.selectable_value(rust_release, true, "release")
                                .on_hover_text(
                                    "cargo build --release --example ... -- 2-15x faster here, \
                                     and what any run worth keeping wants",
                                );
                            if ui
                                .add_enabled(!building, egui::Button::new("compile"))
                                .on_hover_text(if building {
                                    "a compile is already running"
                                } else {
                                    "cargo build for this example"
                                })
                                .clicked()
                            {
                                build_request = true;
                            }
                        });
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
                };
            let config_ui = |ui: &mut egui::Ui| {
                    ui.label(egui::RichText::new("Simulation").strong());
                    ui.separator();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        // One section per field of the simulation -- state,
                        // bodies, camera, and so on -- and the config is one
                        // of those fields, so it is one of those sections
                        // with its own groups inside it.
                        //
                        // What the run *is* comes first -- bodies loaded,
                        // where the camera and Sun are, what the last frame
                        // could see -- then what it was asked to be.
                        let mut sim = sim.borrow_mut();
                        // Passed in rather than read from `sim.config`: that
                        // is the same RefCell this panel is being drawn with
                        // open, and reading it here panics.
                        let c = config.selection_color;
                        simulation_panel::simulation_panel(
                            ui,
                            &mut sim,
                            crate::Vec3::new(c.r as crate::Float, c.g as crate::Float, c.b as crate::Float),
                        );
                        drop(sim);

                        ui.collapsing("Config", |ui| {
                            // Generated from `src/app/config.rs`, so a field
                            // added there gets a widget without anyone
                            // remembering to add one here.
                            config_panel::config_panel(ui, config, app_config);
                        });
                    });
                };

            // Floating panels get their own layers, which egui paints *above*
            // the root -- where side panels and the central panel live.
            //
            // That ordering is why the scene cannot just go in a background
            // `Area` instead: the root layer is painted first whatever order
            // an `Area` asks for, so the scene covered the panels and nothing
            // appeared at any edge, however early the `Area` was created.
            //
            // So the scene stays in the central panel and the panels float
            // over it. In focus mode no side panel takes anything, so the
            // central panel is the whole window and revealing one does not
            // resize the render target.
            let (screen_w, screen_h) = (screen.width(), screen.height());
            let float = |ctx: &egui::Context,
                         id: &'static str,
                         rect: egui::Rect,
                         side: usize,
                         size: &mut f32,
                         resizing: &mut Option<usize>,
                         add: &mut dyn FnMut(&mut egui::Ui)| -> egui::Rect {
                egui::Area::new(id.into())
                    .order(egui::Order::Foreground)
                    .fixed_pos(rect.min)
                    .show(ctx, |ui| {
                        ui.set_max_size(rect.size());
                        let framed = egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.set_min_size(rect.size());
                            add(ui);
                        });

                        // A grab strip on the inner edge, standing in for the
                        // resize handle a docked panel has and an `Area` does
                        // not.
                        //
                        // Measured from where the panel actually ended up, not
                        // from the rect it was asked for: the frame adds its
                        // own margins and a short panel does not fill the
                        // height it was given, so a handle placed from the
                        // request sat away from the edge it belongs to.
                        let actual = framed.response.rect;
                        const GRAB: f32 = 6.0;
                        let strip = match side {
                            0 => egui::Rect::from_min_max(
                                egui::pos2(actual.left(), actual.bottom() - GRAB),
                                actual.max,
                            ),
                            1 => egui::Rect::from_min_max(
                                actual.min,
                                egui::pos2(actual.right(), actual.top() + GRAB),
                            ),
                            2 => egui::Rect::from_min_max(
                                egui::pos2(actual.right() - GRAB, actual.top()),
                                actual.max,
                            ),
                            _ => egui::Rect::from_min_max(
                                actual.min,
                                egui::pos2(actual.left() + GRAB, actual.bottom()),
                            ),
                        };
                        let grab = ui.interact(
                            strip,
                            ui.id().with("grab"),
                            egui::Sense::drag(),
                        );
                        if grab.hovered() || grab.dragged() {
                            ui.ctx().set_cursor_icon(if side < 2 {
                                egui::CursorIcon::ResizeVertical
                            } else {
                                egui::CursorIcon::ResizeHorizontal
                            });
                        }
                        if grab.dragged() {
                            let d = grab.drag_delta();
                            *size += match side {
                                0 => d.y,
                                1 => -d.y,
                                2 => d.x,
                                _ => -d.x,
                            };
                            // Never past the window, and never negative --
                            // dragged shut is a legitimate place to leave one.
                            // Down to nothing is allowed: that is what
                            // putting one away looks like here.
                            let limit = if side < 2 { screen_h } else { screen_w };
                            *size = size.clamp(0.0, limit);
                            *resizing = Some(side);
                        }
                        if grab.drag_stopped() {
                            *resizing = None;
                        }
                    })
                    .response
                    .rect
            };

            // Remembered between reveals, so a panel dragged wider stays
            // wider the next time the pointer summons it.
            let mut sizes = self.float_sizes;
            let mut resizing = self.resizing;

            // A floating panel can be dragged away to nothing, like a docked
            // one. Unlike a docked one it has no handle left behind to drag
            // back -- it is summoned by the pointer instead -- so a panel that
            // was put away comes back at its usual size when it is next
            // asked for. Without this, dragging one to nothing hid it for
            // good: every later reveal showed a panel zero points wide.
            //
            // Only as it reappears, judged by whether it was drawn last
            // frame, or it would spring back under the hand that shrank it.
            for i in 0..4 {
                let reappearing = !was_shown[i];
                if reappearing && sizes[i] < COLLAPSE {
                    sizes[i] = FLOAT_DEFAULTS[i];
                }
            }
            let [top_h, bottom_h, left_w, right_w] = sizes;

            let scene_panel = |ui_root: &mut egui::Ui,
                               vp_rect: &mut egui::Rect,
                               wanted: &mut (u32, u32)| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui_root, |ui| {
                        *vp_rect = ui.available_rect_before_wrap();
                        *wanted = (
                            ((vp_rect.width() * ppp).round() as u32).max(1),
                            ((vp_rect.height() * ppp).round() as u32).max(1),
                        );
                        scene_ui(ui, *vp_rect);
                    });
            };

            if immersive {
                scene_panel(ui_root, &mut vp_rect, &mut wanted);

                let ctx = ui_root.ctx().clone();
                let mut toolbar_ui = toolbar_ui;
                let mut log_ui = log_ui;
                let mut script_ui = script_ui;
                let mut config_ui = config_ui;
                if show_top {
                    let r =
                        egui::Rect::from_min_size(screen.min, egui::vec2(screen.width(), top_h));
                    rects[0] = float(&ctx, "toolbar", r, 0, &mut sizes[0], &mut resizing, &mut toolbar_ui);
                }
                if show_bottom {
                    let r = egui::Rect::from_min_size(
                        egui::pos2(screen.left(), screen.bottom() - bottom_h),
                        egui::vec2(screen.width(), bottom_h),
                    );
                    rects[1] = float(&ctx, "log", r, 1, &mut sizes[1], &mut resizing, &mut log_ui);
                }
                if show_left {
                    let r =
                        egui::Rect::from_min_size(screen.min, egui::vec2(left_w, screen.height()));
                    rects[2] = float(&ctx, "script", r, 2, &mut sizes[2], &mut resizing, &mut script_ui);
                }
                if show_right {
                    let r = egui::Rect::from_min_size(
                        egui::pos2(screen.right() - right_w, screen.top()),
                        egui::vec2(right_w, screen.height()),
                    );
                    rects[3] = float(&ctx, "config", r, 3, &mut sizes[3], &mut resizing, &mut config_ui);
                }
                out_sizes = sizes;
                out_resizing = resizing;
            } else {
                rects[0] = egui::Panel::top("toolbar").show(ui_root, toolbar_ui).response.rect;
                // `show_collapsible`, not `show`: dragging a resize handle
                // past the minimum shuts the panel, and a thin handle stays at
                // the window edge to drag it back -- the way an editor's side
                // bars work. Double clicking the edge toggles it too.
                //
                // `min_size` is doing two jobs, because egui does not let
                // them be separated: it is the smallest a panel can be
                // dragged *and* the size below which it collapses. egui has a
                // `collapse_threshold` for exactly this, but it is private.
                //
                // A comfortable size rather than a small one, tried both
                // ways round and preferred like this: a panel keeps a usable
                // width for as long as it is open, and shutting it is one
                // decisive drag rather than a slow squeeze through sizes
                // nothing can be read at.
                //
                // The floating panels use `COLLAPSE` instead. They are not
                // the same question: those are summoned and dismissed by the
                // pointer already, so shrinking one is about the size it will
                // have next time, not about getting rid of it.
                let mut open = open_docked;
                rects[1] = egui::Panel::bottom("log")
                    .resizable(true)
                    .default_size(bottom_h)
                    .min_size(120.0)
                    .show_collapsible(ui_root, &mut open[1], log_ui)
                    .map(|r| r.response.rect)
                    .unwrap_or(egui::Rect::NOTHING);
                rects[2] = egui::Panel::left("script")
                    .resizable(true)
                    .default_size(left_w)
                    .min_size(180.0)
                    .show_collapsible(ui_root, &mut open[2], script_ui)
                    .map(|r| r.response.rect)
                    .unwrap_or(egui::Rect::NOTHING);
                rects[3] = egui::Panel::right("config")
                    .resizable(true)
                    .default_size(right_w)
                    .min_size(180.0)
                    .show_collapsible(ui_root, &mut open[3], config_ui)
                    .map(|r| r.response.rect)
                    .unwrap_or(egui::Rect::NOTHING);
                out_open = open;
                scene_panel(ui_root, &mut vp_rect, &mut wanted);
            }
        });

        self.viewport_size = wanted;
        self.viewport_rect = vp_rect;
        self.panels = rects;
        self.float_sizes = out_sizes;
        self.resizing = out_resizing;
        self.docked_open = out_open;
        self.run_request |= run_request;
        self.restart_request |= restart_request;
        self.open_request |= open_request;
        self.save_request |= save_request;
        self.build_request |= build_request;
        self.launch_request |= launch_request;
        self.state
            .handle_platform_output(window, output.platform_output);

        let jobs = self
            .ctx
            .tessellate(output.shapes, output.pixels_per_point);
        let desc = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [surface_size.0, surface_size.1],
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
        // Both lists are iterated by reference, which leaves them full, and
        // epaint asserts in debug builds that a delta it handed out was
        // consumed -- so a debug editor panicked on its first frame, on a
        // path a release build never checks.
        output.textures_delta.clear();

        self.viewport_size
    }

    /// Whether a pointer event belongs to the scene rather than the UI.
    ///
    /// True when the pointer is over the viewport and egui is not in the
    /// middle of a drag of its own -- a slider grabbed and dragged across the
    /// viewport keeps belonging to the slider.
    pub fn pointer_on_scene(&self) -> bool {
        if self.ctx.egui_is_using_pointer() {
            return false;
        }
        let Some(p) = self.ctx.pointer_latest_pos() else {
            return false;
        };
        if !self.viewport_rect.contains(p) {
            return false;
        }
        // And not over a panel drawn on top of it. In focus mode the viewport
        // *is* the whole window, so the rect test alone put every panel on the
        // scene's side -- and scrolling the script zoomed the render, which is
        // the thing the docked layout had already been fixed not to do.
        !self.panels.iter().any(|r| r.contains(p))
    }

    /// Give a window event to the UI first.
    ///
    /// Returns true when egui wants it -- a click on a slider, a keystroke in
    /// the script editor -- in which case the camera controller must not also
    /// act on it, or dragging a slider would orbit the scene behind it.
    /// Points per physical pixel, for turning a cursor position into the
    /// coordinates the panel rectangles are in.
    pub fn scale(&self) -> f32 {
        self.ctx.pixels_per_point()
    }

    pub fn on_window_event(
        &mut self,
        window: &winit::window::Window,
        event: &winit::event::WindowEvent,
    ) -> bool {
        self.state.on_window_event(window, event).consumed
    }
}

#[cfg(test)]
mod reveal_tests {
    use super::*;

    fn screen() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1500.0, 925.0))
    }

    fn nothing() -> [egui::Rect; 4] {
        [egui::Rect::NOTHING; 4]
    }

    #[test]
    fn every_panel_shows_when_the_renderer_does_not_own_the_window() {
        let at = Some(egui::pos2(750.0, 500.0));
        assert_eq!(reveal_panels(false, at, screen(), &nothing()), [true; 4]);
        // Even with no pointer at all.
        assert_eq!(reveal_panels(false, None, screen(), &nothing()), [true; 4]);
    }

    #[test]
    fn each_edge_summons_its_own_panel_and_no_other() {
        let s = screen();
        let cases = [
            (egui::pos2(750.0, 4.0), [true, false, false, false]),
            (egui::pos2(750.0, 921.0), [false, true, false, false]),
            (egui::pos2(4.0, 500.0), [false, false, true, false]),
            (egui::pos2(1496.0, 500.0), [false, false, false, true]),
        ];
        for (p, want) in cases {
            assert_eq!(
                reveal_panels(true, Some(p), s, &nothing()),
                want,
                "pointer at {p:?}"
            );
        }
    }

    #[test]
    fn the_middle_summons_nothing() {
        let at = Some(egui::pos2(750.0, 500.0));
        assert_eq!(reveal_panels(true, at, screen(), &nothing()), [false; 4]);
    }

    /// The 24-point strip is far narrower than a panel, so reaching for
    /// anything inside one would dismiss it if only the strip counted.
    #[test]
    fn a_panel_stays_while_the_pointer_is_over_it() {
        let s = screen();
        let mut panels = nothing();
        // The config panel as drawn: 240 wide, down the right-hand side.
        panels[3] = egui::Rect::from_min_max(egui::pos2(1260.0, 0.0), egui::pos2(1500.0, 925.0));

        // Well inside it, and nowhere near the edge strip.
        let deep = egui::pos2(1300.0, 500.0);
        assert!(deep.x < s.right() - EDGE, "the test point must clear the strip");
        assert_eq!(
            reveal_panels(true, Some(deep), s, &panels),
            [false, false, false, true],
            "a panel must stay while the pointer is on it"
        );
    }

    #[test]
    fn no_pointer_shows_nothing_in_focus_mode() {
        assert_eq!(reveal_panels(true, None, screen(), &nothing()), [false; 4]);
    }
}

/// Everything written to stdout and stderr, mirrored into the log panel.
///
/// The panel used to be fed by teeing Python's `sys.stdout`, which caught
/// `print` and tracebacks and nothing else. The renderer's own output --
/// `H` printing the camera, the mesh loader, every `debug_*` flag -- is
/// `println!` from Rust, straight to file descriptor 1, and never went near
/// Python. Pressing `H` and seeing nothing in the log is what that looks
/// like.
///
/// So it is captured a level down, where both end up: the descriptors are
/// pointed at a pipe, and each frame drains it into the log **and** writes it
/// on to the real stdout, so a terminal still shows everything it did.
pub struct StdioCapture {
    reader: std::fs::File,
    /// The original stdout, kept so output still reaches the terminal.
    tty: std::fs::File,
    /// Bytes seen since the last newline.
    partial: String,
}

impl StdioCapture {
    /// The terminal this process started with, for a child to write to.
    ///
    /// A child spawned with inherited stdio writes into the *pipe* this holds
    /// open, which is fine while the editor is here to read it -- and fatal
    /// the moment the editor exits, because the read end goes with it and the
    /// child's next `write` takes a `SIGPIPE`. A launched example outlives
    /// the editor that launched it, so it gets the terminal instead.
    pub fn terminal(&self) -> Option<std::process::Stdio> {
        self.tty.try_clone().ok().map(std::process::Stdio::from)
    }

    /// Redirect stdout and stderr into a pipe. `None` if that fails, in which
    /// case output keeps going to the terminal and the panel stays empty --
    /// worth nobody's run failing over.
    #[cfg(not(unix))]
    pub fn new() -> Option<Self> {
        // Windows has no `dup2` on descriptor 1, and the equivalent
        // (`SetStdHandle` plus a CRT `_dup2`) does not redirect what Rust's
        // `println!` already holds, nor what a Python extension writes
        // through its own CRT. Capture is therefore unavailable here, which
        // is a supported outcome rather than a failure: output keeps going to
        // the terminal and the editor's log panel stays empty. Everything
        // downstream already takes `Option` and handles `None`.
        None
    }

    #[cfg(unix)]
    pub fn new() -> Option<Self> {
        use std::os::fd::{AsRawFd as _, FromRawFd as _};

        let (reader, writer) = std::io::pipe().ok()?;
        // SAFETY: plain descriptor calls. `dup` copies the current stdout so
        // it can be written to afterwards; `dup2` points 1 and 2 at the pipe.
        // A negative return means the redirect did not happen, and the
        // original descriptors are untouched.
        unsafe {
            let saved = libc::dup(libc::STDOUT_FILENO);
            if saved < 0 {
                return None;
            }
            if libc::dup2(writer.as_raw_fd(), libc::STDOUT_FILENO) < 0
                || libc::dup2(writer.as_raw_fd(), libc::STDERR_FILENO) < 0
            {
                libc::dup2(saved, libc::STDOUT_FILENO);
                libc::close(saved);
                return None;
            }
            // Non-blocking, so draining never stalls a frame waiting for a
            // line nobody is going to write.
            let flags = libc::fcntl(reader.as_raw_fd(), libc::F_GETFL);
            libc::fcntl(reader.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK);

            Some(Self {
                reader: std::fs::File::from_raw_fd({
                    let fd = reader.as_raw_fd();
                    std::mem::forget(reader);
                    fd
                }),
                tty: std::fs::File::from_raw_fd(saved),
                partial: String::new(),
            })
        }
    }

    /// Give stdout and stderr back, flushing whatever is still in the pipe.
    ///
    /// Not optional: the tee back to the terminal happens in `drain`, so
    /// anything written after the last frame -- which includes everything a
    /// script prints on its way out -- would be swallowed with the pipe. A
    /// test that printed its result and stopped saw nothing at all.
    #[cfg(not(unix))]
    fn restore(&mut self) {
        // Unreachable: `new` returns `None` on Windows, so no instance
        // exists to drop. Present so the type compiles.
    }

    #[cfg(unix)]
    fn restore(&mut self) {
        use std::io::{Read as _, Write as _};
        use std::os::fd::AsRawFd as _;

        // Descriptors first, so anything printed from here on goes straight
        // out rather than into a pipe nobody will read again.
        // SAFETY: putting back the descriptor saved in `new`.
        unsafe {
            libc::dup2(self.tty.as_raw_fd(), libc::STDOUT_FILENO);
            libc::dup2(self.tty.as_raw_fd(), libc::STDERR_FILENO);
        }

        // Read in the same non-blocking loop `drain` uses. `read_to_end`
        // gives up the moment the pipe would block, which on a pipe with no
        // writer left is immediately -- so the last thing a script printed,
        // the line it exists to report, went nowhere.
        let mut buf = [0u8; 8192];
        loop {
            match self.reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let _ = self.tty.write_all(&buf[..n]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
        if !self.partial.is_empty() {
            let _ = self.tty.write_all(self.partial.as_bytes());
            self.partial.clear();
        }
        let _ = self.tty.flush();
    }

    /// Move whatever has been written since last time into the log.
    pub fn drain(&mut self, log: &mut Log) {
        use std::io::{Read as _, Write as _};

        let mut buf = [0u8; 8192];
        loop {
            match self.reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    // On to the real stdout as well: the terminal is what
                    // survives the window closing.
                    let _ = self.tty.write_all(&buf[..n]);
                    let _ = self.tty.flush();
                    self.partial.push_str(&String::from_utf8_lossy(&buf[..n]));
                    while let Some(i) = self.partial.find('\n') {
                        let line: String = self.partial.drain(..=i).collect();
                        log.push(line.trim_end_matches(['\n', '\r']));
                    }
                }
                // Nothing waiting, which is the usual case.
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
    }
}

impl Drop for StdioCapture {
    fn drop(&mut self) {
        self.restore();
    }
}
