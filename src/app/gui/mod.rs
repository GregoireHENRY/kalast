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

/// How close to an edge the pointer must come to summon a panel, in points.
const EDGE: f32 = 24.0;

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
    viewport_rect: egui::Rect,

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
        state: &mut crate::app::simulation::State,
        shared: &mut crate::app::Shared,
        iteration_rate: f32,
    ) -> (u32, u32) {
        // Re-register only when the texture behind it is a different one. A
        // `TextureId` outlives a resize, but the view it points at does not.
        if self.viewport_texture.is_none()
            || self.registered_size != scene_size
            || self.registered_generation != scene_generation
        {
            let view = scene.create_view(&wgpu::TextureViewDescriptor::default());
            if let Some(id) = self.viewport_texture.take() {
                self.renderer.free_texture(&id);
            }
            self.viewport_texture =
                Some(self.renderer
                    .register_native_texture(device, &view, wgpu::FilterMode::Linear));
            self.registered_size = scene_size;
            self.registered_generation = scene_generation;
        }

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
        let [show_top, show_bottom, show_left, show_right] =
            reveal_panels(immersive, pointer, screen, &self.panels);
        let mut rects = [egui::Rect::NOTHING; 4];
        shared.panels_shown = [show_top, show_bottom, show_left, show_right];
        shared.pointer = pointer.map(|p| (p.x, p.y));
        shared.ui_size = (screen.width(), screen.height());
        let texture_id = self.viewport_texture;
        let script = &mut self.script;
        let script_path = &mut self.script_path;
        let script_dirty = self.script_dirty;
        let script_ran = shared.script_ran;
        let drawn = shared.drawn_iteration;
        let log = &mut shared.log;
        let dirty = &mut self.script_dirty;
        let ran = &mut shared.script_ran;
        let (mut run_request, mut open_request, mut save_request) = (false, false, false);
        let mut restart_request = false;

        let output = self.ctx.run_ui(raw, |ui_root| {
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

            // Added before the panels, not after. Both this and the
            // panels live in egui's background layer, and layers of the
            // same order are painted in the order they were added -- so
            // creating it last drew the scene *over* the panels and they
            // never appeared.
            if immersive {
                // Behind everything, at the full window size, so the panels
                // float *over* the scene instead of taking space from it.
                //
                // A side panel shrinks the central area, which would resize
                // the render target every time one appeared -- reallocating
                // its colour, MSAA and depth textures, and shifting the image
                // under the pointer. In focus mode the scene keeps the whole
                // window and the panels are laid on top.
                vp_rect = screen;
                wanted = (
                    ((screen.width() * ppp).round() as u32).max(1),
                    ((screen.height() * ppp).round() as u32).max(1),
                );
                egui::Area::new("viewport".into())
                    .order(egui::Order::Background)
                    .fixed_pos(screen.min)
                    .show(ui_root.ctx(), |ui| {
                        ui.set_min_size(screen.size());
                        scene_ui(ui, screen);
                    });
            }

            if show_top {
            rects[0] = egui::Panel::top("toolbar").show(ui_root, |ui| {
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
                        .on_hover_text("Rebuild the scene from the script and stop at the start")
                        .clicked()
                    {
                        run_request = true;
                        restart_request = true;
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
                    // What is on screen, not how many have finished. After
                    // the frame for iteration 0 is drawn `state.iteration` is
                    // already 1, and reading "iteration 1" under a picture of
                    // iteration 0 is a lie of exactly one frame.
                    ui.label(format!("iteration {drawn}"))
                        .on_hover_text("The iteration the frame you are looking at was drawn for");
                    let its = if state.is_paused { 0.0 } else { iteration_rate };
                    ui.label(format!("{its:.0} it/s"));
                    ui.weak(format!("{iteration_rate:.0} fps"));
                });
            }).response.rect;
            }

            if show_bottom {
            rects[1] = egui::Panel::bottom("log")
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
                }).response.rect;
            }

            if show_left {
            rects[2] = egui::Panel::left("script")
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
                }).response.rect;
            }

            if show_right {
            rects[3] = egui::Panel::right("config")
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
                        config_panel::config_panel(ui, config, app_config);
                    });
                }).response.rect;
            }


            if !immersive {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui_root, |ui| {
                        let avail = ui.available_size();
                        vp_rect = ui.available_rect_before_wrap();
                        // What the *next* scene render should be, in physical
                        // pixels: egui works in points.
                        wanted = (
                            ((avail.x * ppp).round() as u32).max(1),
                            ((avail.y * ppp).round() as u32).max(1),
                        );
                        scene_ui(ui, vp_rect);
                    });
            }
        });

        self.viewport_size = wanted;
        self.viewport_rect = vp_rect;
        self.panels = rects;
        self.run_request |= run_request;
        self.restart_request |= restart_request;
        self.open_request |= open_request;
        self.save_request |= save_request;
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
        self.ctx
            .pointer_latest_pos()
            .is_some_and(|p| self.viewport_rect.contains(p))
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
