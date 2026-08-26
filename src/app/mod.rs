pub mod body;
pub mod config;
pub mod facet_shadow;
pub mod frame;
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
        let size = winit::dpi::PhysicalSize::new(self.config.width, self.config.height);
        let attrs = winit::window::Window::default_attributes()
            .with_inner_size(size)
            .with_title(&self.config.title);

        let win = Arc::new(ev.create_window(attrs).unwrap());

        self.window = Some(pollster::block_on(crate::app::window::Window::new(
            ev.owned_display_handle(),
            win.clone(),
            &self.config,
            &self.simulation.borrow(),
        )));
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
                let surface_texture = {
                    let win = self.window.as_mut().unwrap();
                    win.window.request_redraw();

                    if !win.is_surface_configured {
                        if self.config.debug_window {
                            println!("[WINDOW] surface is not configured yet")
                        }
                        return;
                    }

                    if let Some(surface_texture) = win.get_surface_texture(&self.config) {
                        surface_texture
                    } else {
                        return;
                    }
                };

                let now = std::time::Instant::now();
                self.dt = (now - self.now).as_secs_f64() as _;
                self.now = now;

                Self::run_callback(&self.before_render, &self.simulation, self.dt);

                {
                    let mut sim = self.simulation.borrow_mut();
                    let win = self.window.as_mut().unwrap();

                    sim.camera
                        .update_with_controller(&mut self.controller, self.dt);

                    win.update(&mut sim, &self.config);
                    win.render(surface_texture, &self.config);

                    // After render: the shadow map now holds this frame's
                    // geometry, so a query here answers for the scene the
                    // tick just set up.
                    if let Some(body) = sim.facet_shadow_request.take() {
                        sim.facet_shadow_result = win.facet_shadow_fractions(body);
                        sim.facet_shadow_body = Some(body);
                    }

                    sim.export_once = false;
                }

                // Outside the borrow above: the callback takes the
                // Simulation itself, so it cannot run while it is held.
                Self::run_callback(&self.after_render, &self.simulation, self.dt);

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

            winit::event::WindowEvent::MouseInput { state, button, .. } => {
                if button == winit::event::MouseButton::Middle {
                    self.controller.middle_pressed = state.is_pressed();
                }
            }

            winit::event::WindowEvent::ModifiersChanged(modifiers) => {
                self.controller.shift_pressed = modifiers.state().shift_key();
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
                    // Arcball only reacts while the middle button is held,
                    // leaving the cursor free for everything else.
                    frame::Control::Arcball if self.controller.middle_pressed => {
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
