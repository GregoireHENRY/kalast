pub mod axes;
pub mod body;
pub mod config;
pub mod facet_id;
pub mod facet_shadow;
pub mod frame;
pub mod hemicube;
pub mod gpu;
pub mod pass;
pub mod simulation;
pub mod uniform;
pub mod window;

use pyo3::prelude::*;
use std::{cell::RefCell, rc::Rc, sync::Arc};

use crate::Float;

pub struct App {
    pub config: crate::app::config::Config,
    pub window: Option<crate::app::window::Window>,

    pub now: std::time::Instant,
    pub dt: Float,

    pub simulation: Rc<RefCell<crate::app::simulation::Simulation>>,
    /// Runs before the frame is drawn: set body transforms, camera and
    /// sun here. Exposed to Python as `before_render` (and `tick`).
    pub before_render: Option<Tick>,
    /// Runs after the frame is drawn, once GPU results for this frame
    /// exist -- notably `Simulation::facet_shadow_result`, which is only
    /// filled once the shadow map holds this frame's geometry.
    ///
    /// Scene changes made here take effect on the *next* frame: the GPU
    /// work for this one is already submitted.
    pub after_render: Option<Tick>,

    pub controller: frame::Controller,

    /// Frames per second for the HUDs, averaged over a fixed window rather
    /// than smoothed per frame. An exponential average still moves every
    /// frame, so the digits churn faster than they can be read; this holds a
    /// value steady for `HUD_RATE_WINDOW` and then replaces it.
    fps_shown: Float,
    fps_window_secs: Float,
    fps_window_frames: u32,
}

/// How long `{fps}` and `{its}` average over before updating, in seconds.
const HUD_RATE_WINDOW: Float = 1.0;

/// Fills one HUD's template in for this frame.
///
/// Deliberately a scan-and-replace rather than a format library: an
/// unrecognised `{name}` is passed through untouched, so a HUD string that
/// happens to contain braces renders instead of erroring or panicking on a
/// user's typo.
///
/// A placeholder may carry a precision, `{fps:.2}`. Rates default to **zero**
/// decimals: a frame rate quoted to a tenth is noise, and the digit changes
/// every update without telling the reader anything.
fn expand_hud(
    template: &str,
    state: &crate::app::simulation::State,
    rate: Float,
) -> String {
    let its = if state.is_paused { 0.0 } else { rate };
    let nit = match state.pause_at {
        Some(n) => n.to_string(),
        None => "?".to_string(),
    };

    // `{name}` or `{name:.N}`; anything else is not a placeholder.
    let split = |key: &str| -> (String, usize) {
        match key.split_once(":.") {
            Some((name, prec)) => match prec.trim_end_matches('f').parse::<usize>() {
                Ok(p) => (name.to_string(), p),
                Err(_) => (key.to_string(), 0),
            },
            None => (key.to_string(), 0),
        }
    };

    let mut out = String::with_capacity(template.len() + 32);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            // Unbalanced: emit the rest verbatim.
            out.push_str(&rest[open..]);
            return out;
        };
        let raw = &after[..close];
        let (name, prec) = split(raw);
        match name.as_str() {
            "it" => out.push_str(&state.iteration.to_string()),
            "nit" => out.push_str(&nit),
            "its" => out.push_str(&format!("{its:.prec$}")),
            "fps" => out.push_str(&format!("{rate:.prec$}")),
            "ms" => {
                let ms = if rate > 0.0 { 1000.0 / rate } else { 0.0 };
                // Milliseconds are the one rate where a decimal earns its
                // place: whole numbers cannot separate 8 ms from 8.4 ms.
                let prec = if raw.contains(":.") { prec } else { 1 };
                out.push_str(&format!("{ms:.prec$}"));
            }
            "paused" => out.push_str(if state.is_paused { "PAUSED" } else { "" }),
            // Unknown: leave it exactly as written.
            _ => {
                out.push('{');
                out.push_str(raw);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

impl App {
    pub fn new() -> Self {
        Self::new_with_config(crate::app::config::Config::default())
    }

    pub fn new_with_config(config: crate::app::config::Config) -> Self {
        let controller = frame::Controller::new(
            config.sensitivity_move,
            config.sensitivity_look,
            config.sensitivity_rotate,
            config.sensitivity_zoom,
        );

        Self {
            config,
            window: None,

            now: std::time::Instant::now(),
            dt: 0.0,

            simulation: Rc::new(RefCell::new(crate::app::simulation::Simulation::new())),
            before_render: None,
            after_render: None,

            controller,
            fps_shown: 0.0,
            fps_window_secs: 0.0,
            fps_window_frames: 0,
        }
    }

    pub fn start(&mut self) {
        self.apply_config_at_start();

        env_logger::init();
        let ev = winit::event_loop::EventLoop::with_user_event()
            .build()
            .unwrap();

        ev.run_app(self).unwrap();
    }

    pub fn apply_config_at_start(&mut self) {
        self.controller.sensitivity_move = self.config.sensitivity_move;
        self.controller.sensitivity_look = self.config.sensitivity_look;
        self.controller.sensitivity_rotate = self.config.sensitivity_rotate;
        self.controller.sensitivity_zoom = self.config.sensitivity_zoom;
        self.controller.emulate_middle_button = self.config.emulate_middle_button;
    }

    pub fn set_tick<F>(&mut self, f: F)
    where
        F: Fn(&mut simulation::Simulation, Float) + 'static,
    {
        self.before_render = Some(Tick::Rust(Box::new(f)));
    }

    pub fn with_tick<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut simulation::Simulation, Float) + 'static,
    {
        self.set_tick(f);
        self
    }

    pub fn set_after_render<F>(&mut self, f: F)
    where
        F: Fn(&mut simulation::Simulation, Float) + 'static,
    {
        self.after_render = Some(Tick::Rust(Box::new(f)));
    }

    /// Invokes one of the two frame callbacks. Both take the same arguments
    /// and differ only in when the app calls them.
    fn run_callback(callback: &Option<Tick>, sim: &Rc<RefCell<simulation::Simulation>>, dt: Float) {
        match callback {
            Some(Tick::Rust(f)) => {
                f(&mut sim.borrow_mut(), dt);
            }
            Some(Tick::Python {
                callback,
                simulation,
            }) => {
                Python::attach(|py: Python<'_>| {
                    callback.call1(py, (simulation.clone(), dt)).unwrap();
                });
            }
            None => {}
        }
    }

    pub fn exit(&mut self, ev: &winit::event_loop::ActiveEventLoop) {
        let win = self.window.as_mut().unwrap();

        if self.simulation.borrow().camera.control == frame::Control::WASD {
            win.reset_cursor();
        }

        // win.get_window().screenshot()

        // Block until every queued/in-flight frame export has actually been
        // written to disk, otherwise anything still in the pipeline when
        // this process exits is silently lost -- there is no resuming a
        // killed background thread.
        let device = win.device.clone();
        win.frame_exporter.finish(&device);

        ev.exit()
    }

    pub fn toggle_export_frame(&mut self) {
        self.window.as_mut().unwrap().toggle_export_frame();
    }
}

impl winit::application::ApplicationHandler<crate::app::window::Window> for crate::app::App {
    fn resumed(&mut self, ev: &winit::event_loop::ActiveEventLoop) {
        // Poll, not Wait. With Wait the loop sleeps until an event arrives,
        // and the only thing that woke it was the redraw it had queued
        // itself -- see `about_to_wait`.
        ev.set_control_flow(winit::event_loop::ControlFlow::Poll);

        let size = winit::dpi::PhysicalSize::new(self.config.width, self.config.height);
        let mut attrs = winit::window::Window::default_attributes()
            .with_inner_size(size)
            .with_title(&self.config.title);

        if self.config.fullscreen {
            // Borderless on the current monitor: `None` means "wherever the
            // window lands", which is what a user pressing the green button
            // would get.
            attrs = attrs.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }

        let win = Arc::new(ev.create_window(attrs).unwrap());

        self.window = Some(pollster::block_on(crate::app::window::Window::new(
            ev.owned_display_handle(),
            win.clone(),
            &self.config,
            &self.simulation.borrow(),
        )));
    }

    /// Keep a redraw pending, every time the event queue empties.
    ///
    /// The redraw chain used to be self-perpetuating: the only
    /// `request_redraw` was *inside* the `RedrawRequested` handler, so each
    /// frame asked for the next. macOS stops delivering redraws to an
    /// occluded window, and a single dropped event therefore broke the chain
    /// for good -- the simulation sat idle indefinitely, and clicking the
    /// window to give it focus was what restarted it, since that made AppKit
    /// issue a redraw of its own.
    ///
    /// Requesting from here instead makes the loop independent of whether the
    /// window is visible. Together with the `Occluded` fix in `window.rs`,
    /// which lets a frame run without a drawable, a covered window now runs at
    /// full speed rather than stopping.
    fn about_to_wait(&mut self, _ev: &winit::event_loop::ActiveEventLoop) {
        if let Some(win) = self.window.as_ref() {
            win.get_window().request_redraw();
        }
    }

    fn window_event(
        &mut self,
        ev: &winit::event_loop::ActiveEventLoop,
        _id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        match event {
            winit::event::WindowEvent::CloseRequested => self.exit(ev),
            winit::event::WindowEvent::Resized(size) => {
                let win = self.window.as_mut().unwrap();
                win.resize(size.width, size.height, &self.config);
            }
            winit::event::WindowEvent::RedrawRequested => {
                {
                    let win = self.window.as_mut().unwrap();
                    win.window.request_redraw();

                    if !win.is_surface_configured {
                        if self.config.debug_window {
                            println!("[WINDOW] surface is not configured yet")
                        }
                        return;
                    }
                }

                let now = std::time::Instant::now();
                self.dt = (now - self.now).as_secs_f64() as _;
                self.now = now;

                self.fps_window_secs += self.dt;
                self.fps_window_frames += 1;
                if self.fps_window_secs >= HUD_RATE_WINDOW {
                    self.fps_shown = self.fps_window_frames as Float / self.fps_window_secs;
                    self.fps_window_secs = 0.0;
                    self.fps_window_frames = 0;
                }

                // Pause has to gate the callbacks, not just the iteration
                // counter. Every Python-driven run puts its physics in
                // `before_render`/`after_render`, so gating only
                // `Simulation::update` -- which does nothing but increment
                // `state.iteration` -- left P with no effect on any of them.
                // The frame itself still runs and still presents, so the
                // window keeps drawing the paused scene and stays responsive
                // to input; only the simulation stops advancing.
                let paused = self.simulation.borrow().state.is_paused;

                if !paused {
                    Self::run_callback(&self.before_render, &self.simulation, self.dt);
                }

                {
                    let mut sim = self.simulation.borrow_mut();
                    let win = self.window.as_mut().unwrap();

                    sim.camera
                        .update_with_controller(&mut self.controller, self.dt);

                    win.update(&mut sim, &self.config);

                    // The HUDs are shared handles, so this reads whatever
                    // `before_render` just wrote into them. Only the text is
                    // expanded; position, size and colour are used as they
                    // stand.
                    let huds: Vec<crate::app::config::Hud> = sim
                        .huds
                        .iter()
                        .map(|h| {
                            let h = h.borrow();
                            crate::app::config::Hud {
                                text: expand_hud(&h.text, &sim.state, self.fps_shown),
                                ..h.clone()
                            }
                        })
                        .collect();

                    // Acquired here, not at the top of the frame.
                    //
                    // A drawable is a scarce resource -- the surface is
                    // configured for two frames of latency -- and holding one
                    // across `before_render` meant holding it across
                    // arbitrary user Python: SPICE lookups, a TPM step,
                    // whatever the script does. In a native-fullscreen window
                    // that starved the pool until `nextDrawable` hit its
                    // one-second timeout, measured at 1001 ms and 3725 ms of
                    // `acquire drawable` while the same window merely
                    // maximised was fine.
                    //
                    // Occlusion is still deliberately not an early return: an
                    // occluded window yields no drawable, and skipping the
                    // frame on that basis halted the simulation outright
                    // rather than just not drawing it. The frame runs either
                    // way; only the present is skipped.
                    let surface_texture = win.get_surface_texture(&self.config);
                    win.render(surface_texture, &self.config, &huds);

                    // After render: the shadow map now holds this frame's
                    // geometry, so a query here answers for the scene
                    // before_render just set up.
                    let one_off = sim.facet_shadow_request.take();
                    if self.config.access_shadow_map || one_off.is_some() {
                        let n = sim.bodies.len();
                        sim.facet_shadow_result.resize(n, vec![]);

                        for body in 0..n {
                            let wanted =
                                self.config.access_shadow_map || one_off == Some(body);
                            if wanted {
                                sim.facet_shadow_result[body] =
                                    win.facet_shadow_fractions(body);
                            } else {
                                // Stale results would silently describe an
                                // older frame's geometry.
                                sim.facet_shadow_result[body].clear();
                            }
                        }
                    } else if !sim.facet_shadow_result.is_empty() {
                        sim.facet_shadow_result.clear();
                    }

                    // Same reasoning as the shadow query: the ID pass draws
                    // the scene the callbacks just positioned, so it belongs
                    // after the render, and its result is dropped when not
                    // requested rather than left to describe an older frame.
                    if let Some((body, facets, res, batch)) = sim.hemicube_request.take() {
                        let mesh = sim
                            .bodies
                            .get(body)
                            .and_then(|b| b.mesh.as_ref())
                            .map(|m| m.borrow().clone());
                        // Same scene fit the shadow pass uses: the frustum has
                        // to cover the companion, not just the body it sits on.
                        let scene = sim.scene_bounds();
                        sim.hemicube_result = mesh.map(|m| {
                            let (rows, offsets, n_total) =
                                win.hemicube_rows(body, &m, scene, &facets, res, batch);
                            (rows, facets.len(), n_total as usize, offsets)
                        });
                    } else {
                        sim.hemicube_result = None;
                    }

                    if sim.facet_id_request {
                        sim.facet_id_request = false;
                        sim.facet_id_result = Some(win.facet_id_map());
                    } else {
                        sim.facet_id_result = None;
                    }

                    sim.export_once = false;
                }

                // Outside the borrow above: the callback takes the
                // Simulation itself, so it cannot run while it is held.
                if !paused {
                    Self::run_callback(&self.after_render, &self.simulation, self.dt);
                }

                // Advance only now that both callbacks have run, so they
                // agree on which frame they are in -- a loop deriving an
                // epoch from `state.iteration` would otherwise see two
                // different times within one frame.
                self.simulation.borrow_mut().update();

            }

            winit::event::WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        physical_key: winit::keyboard::PhysicalKey::Code(code),
                        state: key_state,
                        ..
                    },
                ..
            } => {
                let is_pressed = key_state.is_pressed();
                self.controller.handle_key(code, is_pressed);

                match (code, is_pressed) {
                    (winit::keyboard::KeyCode::Escape, true) => self.exit(ev),
                    (winit::keyboard::KeyCode::Space, true) => {
                        // let win = self.window.as_mut().unwrap();
                        // win.toggle_color_xy = !win.toggle_color_xy;
                    }
                    (winit::keyboard::KeyCode::KeyP, true) => {
                        let pause = self.simulation.borrow_mut().state.toggle_pause();
                        if self.config.debug_app {
                            println!("[APP] Simulation paused={}", pause);
                        }
                    }

                    (winit::keyboard::KeyCode::KeyT, true) => {
                        // switch camera type
                        self.simulation.borrow_mut().camera.control.toggle();
                        let control = self.simulation.borrow().camera.control;
                        if self.config.debug_app {
                            println!("[APP] Camera control changed, now is {:?}", control);
                        }
                        match control {
                            frame::Control::Arcball => {
                                // reset cursor middle
                                let win = self.window.as_ref().unwrap();
                                win.reset_cursor();
                            }
                            frame::Control::WASD => {
                                // no cursor in WASD
                                let win = self.window.as_ref().unwrap();
                                win.center_cursor();
                                win.window.set_cursor_visible(false);
                                win.window
                                    .set_cursor_grab(winit::window::CursorGrabMode::Confined)
                                    .or_else(|_e| {
                                        win.window
                                            .set_cursor_grab(winit::window::CursorGrabMode::Locked)
                                    })
                                    .unwrap();
                            }
                            frame::Control::None => {}
                        }
                    }

                    (winit::keyboard::KeyCode::KeyH, true) => {
                        println!(
                            "camera: pos={} up={} dir={} anchor={} projection={:?}",
                            self.simulation.borrow().camera.pos,
                            self.simulation.borrow().camera.up,
                            self.simulation.borrow().camera.dir,
                            self.simulation.borrow().camera.anchor,
                            self.simulation.borrow().camera.projection
                        );
                    }

                    _ => {}
                };
            }

            winit::event::WindowEvent::PinchGesture { delta, .. } => {
                if self.simulation.borrow().camera.control == frame::Control::Arcball {
                    self.controller.zoom(delta as Float);
                }
            }

            winit::event::WindowEvent::MouseInput { state, button, .. } => match button {
                winit::event::MouseButton::Middle => {
                    self.controller.middle_pressed = state.is_pressed();
                }
                winit::event::MouseButton::Left => {
                    self.controller.left_pressed = state.is_pressed();
                }
                _ => {}
            },

            winit::event::WindowEvent::ModifiersChanged(modifiers) => {
                self.controller.shift_pressed = modifiers.state().shift_key();
                self.controller.alt_pressed = modifiers.state().alt_key();
            }

            _ => {}
        };
    }

    fn device_event(
        &mut self,
        _ev_loop: &winit::event_loop::ActiveEventLoop,
        _id: winit::event::DeviceId,
        ev: winit::event::DeviceEvent,
    ) {
        match ev {
            winit::event::DeviceEvent::MouseMotion { delta: (dx, dy) } => {
                match self.simulation.borrow().camera.control {
                    // WASD grabs the cursor, so every motion is a look.
                    frame::Control::WASD => {
                        self.controller.mouse_motion(dx as Float, dy as Float);
                    }
                    // Arcball only reacts during a drag -- middle button, or
                    // alt + left where there is no middle button to press --
                    // leaving the cursor free for everything else.
                    frame::Control::Arcball if self.controller.is_dragging() => {
                        self.controller.drag(dx as Float, dy as Float);
                    }
                    _ => {}
                }
            }

            winit::event::DeviceEvent::MouseWheel { delta } => {
                // A wheel reports discrete notches, a trackpad reports
                // pixels. Normalising them here is what lets one sensitivity
                // constant feel right on both -- previously a notch was
                // multiplied by 100 and fed to rotation, so a mouse could
                // only spin the camera in huge single-axis jumps.
                let notches = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, dy) => dy as Float,
                    winit::event::MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition {
                        y,
                        ..
                    }) => y as Float / 50.0,
                };

                if self.simulation.borrow().camera.control == frame::Control::Arcball {
                    self.controller.zoom(notches);
                }
            }
            _ => {}
        };
    }
}

pub enum Tick {
    Rust(Box<dyn for<'a> Fn(&'a mut simulation::Simulation, Float)>),
    Python {
        callback: Py<PyAny>,
        simulation: crate::py::app::simulation::Simulation,
    },
}

#[cfg(test)]
mod hud_tests {
    use super::*;
    use crate::app::simulation::State;

    fn state(iteration: usize, paused: bool, pause_at: Option<usize>) -> State {
        let mut s = State::new();
        s.iteration = iteration;
        s.is_paused = paused;
        s.pause_at = pause_at;
        s
    }

    #[test]
    fn expands_the_documented_placeholders() {
        let s = state(42, false, Some(500));
        assert_eq!(
            expand_hud("{it}/{nit} ({its} it/s)", &s, 60.4),
            "42/500 (60 it/s)"
        );
    }

    /// Rates are whole numbers unless asked otherwise: a tenth of a frame per
    /// second is noise, and the digit churns without informing anyone.
    #[test]
    fn rates_are_integers_by_default_and_precision_is_opt_in() {
        let s = state(1, false, None);
        assert_eq!(expand_hud("{fps}", &s, 59.62), "60");
        assert_eq!(expand_hud("{fps:.1}", &s, 59.62), "59.6");
        assert_eq!(expand_hud("{fps:.2f}", &s, 59.62), "59.62");
    }

    /// Milliseconds are the exception -- whole numbers cannot separate 8 from
    /// 8.4 ms, which is the difference between hitting and missing 120 Hz.
    #[test]
    fn milliseconds_keep_one_decimal_by_default() {
        let s = state(1, false, None);
        assert_eq!(expand_hud("{ms}", &s, 120.0), "8.3");
        assert_eq!(expand_hud("{ms:.0}", &s, 120.0), "8");
    }

    /// `?` rather than a made-up number: the run length is genuinely unknown
    /// unless something has been told to stop at it.
    #[test]
    fn unknown_run_length_reads_as_a_question_mark() {
        assert_eq!(expand_hud("{nit}", &state(1, false, None), 60.0), "?");
    }

    /// The counter is not advancing while paused, so reporting the frame rate
    /// as an iteration rate would be a lie. `{fps}` still reports frames.
    #[test]
    fn iteration_rate_is_zero_while_paused_but_frame_rate_is_not() {
        let s = state(7, true, None);
        assert_eq!(expand_hud("{its}|{fps}|{paused}", &s, 120.0), "0|120|PAUSED");
    }

    /// A typo must render, not panic or swallow the text around it.
    #[test]
    fn unknown_and_unbalanced_braces_pass_through() {
        let s = state(1, false, None);
        assert_eq!(expand_hud("{nope} x {it}", &s, 60.0), "{nope} x 1");
        assert_eq!(expand_hud("a {unclosed", &s, 60.0), "a {unclosed");
        assert_eq!(expand_hud("no braces", &s, 60.0), "no braces");
    }

}
