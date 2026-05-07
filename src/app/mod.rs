pub mod body;
pub mod config;
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
    pub tick: Option<Tick>,

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
            tick: None,

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
        self.tick = Some(Tick::Rust(Box::new(f)));
    }

    pub fn with_tick<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut simulation::Simulation, Float) + 'static,
    {
        self.set_tick(f);
        self
    }

    pub fn exit(&self, ev: &winit::event_loop::ActiveEventLoop) {
        let win = self.window.as_ref().unwrap();

        if self.simulation.borrow().camera.control == frame::Control::WASD {
            win.reset_cursor();
        }

        // win.get_window().screenshot()

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
                let texture = {
                    let win = self.window.as_mut().unwrap();
                    win.window.request_redraw();

                    if !win.is_surface_configured {
                        if self.config.debug_window {
                            println!("[WINDOW] surface is not configured yet")
                        }
                        return;
                    }

                    if let Some(texture) = win.get_surface_texture(&self.config) {
                        texture
                    } else {
                        return;
                    }
                };

                let now = std::time::Instant::now();
                self.dt = (now - self.now).as_secs_f64() as _;
                self.now = now;

                match &self.tick {
                    Some(Tick::Rust(f)) => {
                        f(&mut self.simulation.borrow_mut(), self.dt);
                    }
                    Some(Tick::Python {
                        callback,
                        simulation,
                    }) => {
                        Python::attach(|py: Python<'_>| {
                            callback.call1(py, (simulation.clone(), self.dt)).unwrap();
                        });
                    }
                    None => {}
                };

                {
                    let mut sim = self.simulation.borrow_mut();
                    let win = self.window.as_mut().unwrap();

                    sim.camera
                        .update_with_controller(&mut self.controller, self.dt);

                    sim.update();
                    win.update(&sim, &self.config);
                    win.render(texture, &self.config);

                    sim.export_once = false;
                }
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

                    _ => {}
                };
            }

            winit::event::WindowEvent::PinchGesture { delta, .. } => {
                if self.simulation.borrow().camera.control == frame::Control::Arcball {
                    self.controller.zoom(delta as Float);
                }
            }

            winit::event::WindowEvent::MouseInput {
                state: _state,
                button: _button,
                ..
            } => {}

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
                if self.simulation.borrow().camera.control == frame::Control::WASD {
                    self.controller.mouse_motion(dx as Float, dy as Float);
                }
            }

            winit::event::DeviceEvent::MouseWheel { delta } => {
                let (dx, dy) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(dx, dy) => {
                        (dx as Float * 100.0, dy as Float * 100.0)
                    }
                    winit::event::MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition {
                        x,
                        y,
                    }) => (x as Float, y as Float),
                };

                if self.simulation.borrow().camera.control == frame::Control::Arcball {
                    self.controller.mouse_motion(-dx, -dy);
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
