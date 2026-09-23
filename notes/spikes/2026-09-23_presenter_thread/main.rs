// Spike: does wgpu 30 let one thread render continuously while another
// acquires, copies and presents? Main thread: winit loop, a render pass per
// frame into `frame` texture. Presenter thread: get_current_texture (blocks),
// copy `frame` into the drawable, submit, present. Prints per second.
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    frame: wgpu::Texture,
}

struct App {
    window: Option<Arc<winit::window::Window>>,
    gpu: Option<Gpu>,
    frames: u32,
    since: Instant,
    started: Instant,
    presents: Arc<AtomicU32>,
    max_acquire_us: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    t: f32,
    mode: String,
    mode_wait: bool,
    hold_redraw: bool,
    last_redraw: Instant,
}

impl winit::application::ApplicationHandler for App {
    fn resumed(&mut self, ev: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        ev.set_control_flow(winit::event_loop::ControlFlow::Poll);
        let window = Arc::new(
            ev.create_window(
                winit::window::Window::default_attributes()
                    .with_inner_size(winit::dpi::LogicalSize::new(640, 480))
                    .with_title("spike"),
            )
            .unwrap(),
        );
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(Box::new(ev.owned_display_handle())));
        let surface = Arc::new(instance.create_surface(window.clone()).unwrap());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let size = window.inner_size();
        let mut config = surface.get_default_config(&adapter, size.width, size.height).unwrap();
        config.present_mode = wgpu::PresentMode::Immediate;
        config.usage |= wgpu::TextureUsages::COPY_DST;
        surface.configure(&device, &config);
        let frame = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame"),
            size: wgpu::Extent3d { width: size.width, height: size.height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        println!("format {:?}, present modes {:?}", config.format, surface.get_capabilities(&adapter).present_modes);

        // The presenter thread.
        let (d, q, f, w) = (device.clone(), queue.clone(), frame.clone(), window.clone());
        let (presents, max_acq, stop) = (self.presents.clone(), self.max_acquire_us.clone(), self.stop.clone());
        let mode = self.mode.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let t0 = Instant::now();
                match surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(drawable) => {
                        max_acq.fetch_max(t0.elapsed().as_micros() as u64, Ordering::Relaxed);
                        let mut enc = d.create_command_encoder(&Default::default());
                        if mode == "copy" {
                            enc.copy_texture_to_texture(
                                f.as_image_copy(),
                                drawable.texture.as_image_copy(),
                                wgpu::Extent3d { width: f.width().min(drawable.texture.width()), height: f.height().min(drawable.texture.height()), depth_or_array_layers: 1 },
                            );
                        } else {
                            // "clear": render into the drawable directly.
                            let view = drawable.texture.create_view(&Default::default());
                            let _p = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &view, depth_slice: None, resolve_target: None,
                                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.1, g: 0.6, b: 0.2, a: 1.0 }), store: wgpu::StoreOp::Store },
                                })],
                                ..Default::default()
                            });
                        }
                        q.submit([enc.finish()]);
                        w.pre_present_notify();
                        q.present(drawable);
                        presents.fetch_add(1, Ordering::Relaxed);
                    }
                    wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => std::thread::sleep(Duration::from_millis(2)),
                    other => { eprintln!("presenter: {:?}", std::mem::discriminant(&other)); std::thread::sleep(Duration::from_millis(5)); }
                }
            }
        });
        self.gpu = Some(Gpu { device, queue, frame });
        self.window = Some(window);
    }

    fn about_to_wait(&mut self, ev: &winit::event_loop::ActiveEventLoop) {
        if self.started.elapsed() > Duration::from_secs(8) {
            self.stop.store(true, Ordering::Relaxed);
            ev.exit();
            return;
        }
        if let Some(w) = &self.window {
            if self.hold_redraw {
                // the idle pump: no frame
            } else if self.mode_wait {
                if self.last_redraw.elapsed() >= Duration::from_millis(5) {
                    self.last_redraw = Instant::now();
                    w.request_redraw();
                }
            } else {
                w.request_redraw();
            }
        }
    }

    fn window_event(&mut self, ev: &winit::event_loop::ActiveEventLoop, _id: winit::window::WindowId, event: winit::event::WindowEvent) {
        match event {
            winit::event::WindowEvent::CloseRequested => ev.exit(),
            winit::event::WindowEvent::RedrawRequested => {
                let Some(gpu) = &self.gpu else { return };
                self.t += 0.01;
                let view = gpu.frame.create_view(&Default::default());
                let mut enc = gpu.device.create_command_encoder(&Default::default());
                {
                    let _p = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view, depth_slice: None, resolve_target: None,
                            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: (self.t.sin() * 0.5 + 0.5) as f64, g: 0.2, b: (self.t.cos() * 0.5 + 0.5) as f64, a: 1.0 }), store: wgpu::StoreOp::Store },
                        })],
                        ..Default::default()
                    });
                }
                gpu.queue.submit([enc.finish()]);
                self.frames += 1;
                if self.since.elapsed() >= Duration::from_secs(1) {
                    println!("{} frames/s, {} presents/s, longest acquire {:.1} ms", self.frames, self.presents.swap(0, Ordering::Relaxed), self.max_acquire_us.swap(0, Ordering::Relaxed) as f64 / 1000.0);
                    self.frames = 0;
                    self.since = Instant::now();
                }
                // A little work so the loop is not pure submit.
                std::thread::sleep(Duration::from_micros(300));
            }
            _ => {}
        }
    }
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "copy".into());
    let ev = winit::event_loop::EventLoop::new().unwrap();
    let mut app = App {
        window: None, gpu: None, frames: 0, since: Instant::now(), started: Instant::now(),
        presents: Arc::new(AtomicU32::new(0)), max_acquire_us: Arc::new(AtomicU64::new(0)),
        stop: Arc::new(AtomicBool::new(false)), t: 0.0, mode,
        mode_wait: std::env::args().nth(2).as_deref() == Some("wait"), last_redraw: Instant::now(),
        hold_redraw: false,
    };
    if std::env::args().nth(2).as_deref() == Some("pump2") {
        // kalast's step() with an idle pump after the frame: the run loop
        // gets a millisecond to service the presenter, then Python runs.
        use winit::platform::pump_events::EventLoopExtPumpEvents;
        let mut ev = ev;
        let idle_us: u64 = std::env::args().nth(3).map(|s| s.parse().unwrap()).unwrap_or(1000);
        while app.started.elapsed() < Duration::from_secs(8) {
            app.hold_redraw = false;
            if let winit::platform::pump_events::PumpStatus::Exit(_) = ev.pump_app_events(Some(Duration::ZERO), &mut app) { break; }
            app.hold_redraw = true;
            if let winit::platform::pump_events::PumpStatus::Exit(_) = ev.pump_app_events(Some(Duration::from_micros(idle_us)), &mut app) { break; }
            std::thread::sleep(Duration::from_millis(3)); // the script's work
        }
        app.stop.store(true, Ordering::Relaxed);
    } else if std::env::args().nth(2).as_deref() == Some("wait") {
        // The main thread idles *inside* the run loop between frames: pump
        // with a timeout, a redraw at most every 5 ms.
        use winit::platform::pump_events::EventLoopExtPumpEvents;
        let mut ev = ev;
        while app.started.elapsed() < Duration::from_secs(8) {
            if let winit::platform::pump_events::PumpStatus::Exit(_) =
                ev.pump_app_events(Some(Duration::from_millis(5)), &mut app)
            {
                break;
            }
        }
        app.stop.store(true, Ordering::Relaxed);
    } else if std::env::args().nth(2).as_deref() == Some("pump") {
        // kalast's step(): one frame per pump, the run loop not running in
        // between -- here the "script work" is a short sleep.
        use winit::platform::pump_events::EventLoopExtPumpEvents;
        let mut ev = ev;
        while app.started.elapsed() < Duration::from_secs(8) {
            if let winit::platform::pump_events::PumpStatus::Exit(_) =
                ev.pump_app_events(Some(Duration::ZERO), &mut app)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        app.stop.store(true, Ordering::Relaxed);
    } else {
        ev.run_app(&mut app).unwrap();
    }
    println!("done");
}
