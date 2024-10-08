use crate::{
    intersect_asteroids, util::*, win::WindowScene, AirlessBody, GraphicalPipeline, MovementMode,
    ProjectionMode, Shader, Surface, WindowSettings, WindowState,
    SENSITIVITY_ROTATE_MOUSEWHEEL_CORRECTION, SPEED, SPEED_FAST_FACTOR,
};

use egui_sdl2_gl::egui::ahash::HashMapExt;
use sdl2;
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::keyboard::Mod;
use sdl2::mouse::MouseButton;
use sdl2::rect::Rect;
use sdl2::sys as sdl2_sys;
use sdl2::video::Window as SDL_Window;
use sdl2::video::{FullscreenType, GLContext};
use sdl2::video::{GLProfile, SwapInterval};
use sdl2::Sdl;

use gl::types::*;

use egui_backend::egui;
use egui_backend::{DpiScaling, ShaderVersion};
// Alias the backend to something less mouthful
use egui_backend::egui::FullOutput;
use egui_sdl2_gl::egui::{Context, Ui, ViewportIdMap, ViewportOutput};
use egui_sdl2_gl::painter::Painter;
use egui_sdl2_gl::{self as egui_backend, EguiStateHandler};

use std::borrow::Borrow;
use std::cell::RefCell;
use std::ffi::CStr;
use std::fmt::Debug;
use std::fs;
use std::path::Path;
use std::time::Instant;

use itertools::{izip, Itertools};

#[allow(unused)]
fn viewport(width: u32, height: u32) {
    unsafe { gl::Viewport(0, 0, width as _, height as _) }
}

fn viewport_adaptative(width: u32, height: u32, diff_height: i32) {
    unsafe { gl::Viewport(0, diff_height as _, width as _, (height as i32) as _) }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameEvent {
    Continue,
    Exit,
    None,
}

#[derive(Debug)]
pub struct Clock {
    pub start: Instant,
    pub now: RefCell<u64>,
}

impl Clock {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            now: RefCell::new(unsafe { sdl2_sys::SDL_GetPerformanceCounter() }),
        }
    }

    pub fn delta_time(&self) -> Float {
        let last = *self.now.borrow();
        let now = unsafe { sdl2_sys::SDL_GetPerformanceCounter() };

        let dt = (now - last) as Float
            / unsafe { sdl2_sys::SDL_GetPerformanceFrequency() } as Float
            * 1000.0;

        *self.now.borrow_mut() = now;

        dt
    }

    pub fn elapsed(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}

pub struct EGui {
    pub painter: Painter,
    pub state: EguiStateHandler,
    pub ctx: Context,
    pub viewport_output: ViewportIdMap<ViewportOutput>,
}

impl EGui {
    pub fn new(win: &SDL_Window) -> Self {
        let shader_ver = ShaderVersion::Default;
        let (painter, state) = egui_backend::with_sdl2(&win, shader_ver, DpiScaling::Default);
        let ctx = egui::Context::default();
        let viewport_output = ViewportIdMap::new();

        Self {
            painter,
            state,
            ctx,
            viewport_output,
        }
    }
}

pub struct RawWindow {
    pub sdl: Sdl,
    pub win: SDL_Window,
    pub ctx: GLContext,
    pub egui: EGui,
}

impl Debug for RawWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RawWindow")
    }
}

impl RawWindow {
    pub fn new(settings: &WindowSettings) -> Self {
        let sdl = sdl2::init().unwrap();
        let video = sdl.video().unwrap();

        let attr = video.gl_attr();
        attr.set_context_profile(GLProfile::Core);
        attr.set_context_version(settings.gl_version.0, settings.gl_version.1);

        #[cfg(target_os = "macos")]
        attr.set_context_flags().forward_compatible().set();

        if let Some(multisampling) = settings.multisampling {
            attr.set_multisample_buffers(1);
            attr.set_multisample_samples(multisampling);
        }

        let mut winbuild = video.window("Kalast", settings.width as u32, settings.height as u32);

        winbuild.opengl().position_centered().resizable();

        if settings.is_high_dpi() {
            winbuild.allow_highdpi();
        }

        if settings.fullscreen == FullscreenType::True {
            winbuild.fullscreen();
        } else if settings.fullscreen == FullscreenType::Desktop {
            winbuild.fullscreen_desktop();
        }

        let win = winbuild.build().unwrap();

        let ctx = win.gl_create_context().unwrap();
        gl::load_with(|name| video.gl_get_proc_address(name) as _);

        let egui = EGui::new(&win);

        if settings.vsync {
            video.gl_set_swap_interval(SwapInterval::VSync).unwrap();
        } else {
            video.gl_set_swap_interval(SwapInterval::Immediate).unwrap();
        }

        unsafe {
            gl::Enable(gl::DEPTH_TEST);
            gl::DepthFunc(gl::LESS);
            gl::Enable(gl::MULTISAMPLE);

            if settings.face_culling {
                gl::Enable(gl::CULL_FACE);
            }

            gl::Enable(gl::LINE_SMOOTH);
        };

        if settings.debug {
            for (name, var) in izip!(
                ["GL Vendor", "GL Renderer", "GL Version", "GLSL Version"],
                [
                    gl::VENDOR,
                    gl::RENDERER,
                    gl::VERSION,
                    gl::SHADING_LANGUAGE_VERSION
                ]
            ) {
                unsafe {
                    println!(
                        "{name}: {}",
                        CStr::from_ptr(gl::GetString(var) as _).to_str().unwrap()
                    );
                }
            }
        }

        // sdl.mouse().set_relative_mouse_mode(true);
        // sdl.mouse().show_cursor(true);

        sdl.mouse().warp_mouse_in_window(
            &win,
            (settings.width / 2) as i32,
            (settings.height / 2) as i32,
        );

        Self {
            sdl,
            win,
            ctx,
            egui,
        }
    }
}

#[derive(Debug)]
pub struct Window {
    pub win: RawWindow,
    pub settings: RefCell<WindowSettings>,
    pub state: RefCell<WindowState>,
    pub clock: Clock,
    pub graphical_pipeline: GraphicalPipeline,
    pub scene: RefCell<WindowScene>,
}

impl Window {
    pub fn new(settings: WindowSettings) -> Self {
        let win = RawWindow::new(&settings);
        let state = WindowState::default();

        let graphical_pipeline = GraphicalPipeline::new(&settings);
        let clock = Clock::new();
        let scene = WindowScene::new(&settings);

        Self {
            win,
            settings: RefCell::new(settings),
            state: RefCell::new(state),
            clock,
            graphical_pipeline,
            scene: RefCell::new(scene),
        }
    }

    pub fn with_settings<F>(modifier: F) -> Self
    where
        F: Fn(&mut WindowSettings),
    {
        let mut settings = WindowSettings::default();
        modifier(&mut settings);
        Self::new(settings)
    }

    pub fn load_surfaces<'a, I>(&mut self, surfaces: I)
    where
        I: IntoIterator<Item = &'a Surface>,
    {
        {
            let mut scene = self.scene.borrow_mut();
            scene.load_surfaces(surfaces);
        }
    }

    pub fn with_surfaces<'a, I>(self, surfaces: I) -> Self
    where
        I: IntoIterator<Item = &'a Surface>,
    {
        let mut win = self;
        win.load_surfaces(surfaces);
        win
    }

    pub fn with_surface(self, surf: &Surface) -> Self {
        {
            let mut scene = self.scene.borrow_mut();
            scene.load_surface(surf);
        }
        self
    }

    pub fn load_trajectory(self, points: &[Vec3]) -> Self {
        {
            let mut scene = self.scene.borrow_mut();
            scene.load_trajectory(points);
        }
        self
    }

    pub fn update_vao(&self, body_index: usize, surf: &mut Surface) {
        let mut scene = self.scene.borrow_mut();
        scene.update_surface_data(body_index, surf);
    }

    pub fn update_vaos<'a, I>(&self, surfaces: I)
    where
        I: IntoIterator<Item = &'a mut Surface>,
    {
        let mut scene = self.scene.borrow_mut();
        for (ii, surf) in surfaces.into_iter().enumerate() {
            scene.update_surface_data(ii, surf);
        }
    }

    pub fn toggle_pause(&self) {
        self.state.borrow_mut().toggle_pause();
    }

    pub fn toggle_fullscreen(&mut self) {
        let mode = self.settings.borrow_mut().toggle_fullscreen();
        self.win.win.set_fullscreen(mode).unwrap();
    }

    pub fn toggle_fullscreen_windowed(&mut self) {
        let mode = self.settings.borrow_mut().toggle_fullscreen_windowed();
        self.win.win.set_fullscreen(mode).unwrap();
    }

    pub fn toggle_debug(&self) {
        self.settings.borrow_mut().toggle_debug();
    }

    pub fn is_paused(&self) -> bool {
        self.state.borrow_mut().pause
    }

    pub fn export_quit(&self) {
        self.state.borrow_mut().export_quit()
    }

    pub fn is_export_quit(&self) -> bool {
        self.state.borrow().export_quit
    }

    pub fn picked(&self) -> Option<(usize, usize)> {
        self.graphical_pipeline.picker.borrow_mut().picked_on
    }

    pub fn picked_take(&self) -> Option<(usize, usize)> {
        self.graphical_pipeline.picker.borrow_mut().picked_on.take()
    }

    pub fn ticks(&self) -> u32 {
        unsafe { sdl2_sys::SDL_GetTicks() }
    }

    pub fn clear(&self, mask: GLbitfield) {
        unsafe {
            gl::Clear(mask);
        }
    }

    pub fn clear_with_color(&self, red: f32, green: f32, blue: f32, alpha: f32) {
        unsafe {
            gl::ClearColor(red, green, blue, alpha);
        }
    }

    pub fn clear_with_background(&self) {
        let color: glm::Vec3 = glm::convert(self.settings.borrow().background_color);
        self.clear_with_color(color.x, color.y, color.z, 1.0);
    }

    pub fn update_mapshadow(&self) {
        let settings = self.settings.borrow();
        self.graphical_pipeline.mapshadow.update(&settings);
    }

    pub fn center_cursor(&self) {
        let settings = self.settings.borrow();
        self.win.sdl.mouse().warp_mouse_in_window(
            &self.win.win,
            (settings.width / 2) as i32,
            (settings.height / 2) as i32,
        );
    }

    pub fn warp_mouse_in_window_repeat(&self, x: i32, y: i32) {
        let settings = self.settings.borrow();
        if x >= settings.width as _ {
            self.win
                .sdl
                .mouse()
                .warp_mouse_in_window(&self.win.win, 1, y);
        }
        if x <= 0 as _ {
            self.win
                .sdl
                .mouse()
                .warp_mouse_in_window(&self.win.win, (settings.width - 1) as _, y);
        }
        if y >= settings.height as _ {
            self.win
                .sdl
                .mouse()
                .warp_mouse_in_window(&self.win.win, x, 1);
        }
        if y <= 0 as _ {
            self.win
                .sdl
                .mouse()
                .warp_mouse_in_window(&self.win.win, x, (settings.height - 1) as _);
        }
    }

    pub fn events(&mut self) -> FrameEvent {
        let clock_delta_time = self.clock.delta_time();
        let mut event_pump = self.win.sdl.event_pump().unwrap();

        let (forward, left, backward, right) = {
            let s = self.settings.borrow();
            (s.forward, s.left, s.backward, s.right)
        };

        if self.state.borrow().quit {
            return FrameEvent::Exit;
        }

        for event in event_pump.poll_iter() {
            match event {
                Event::Window {
                    timestamp,
                    window_id,
                    win_event,
                } => {
                    if self.settings.borrow().debug {
                        println!("Event Window: {} {} {:?}", timestamp, window_id, win_event);
                    }

                    match win_event {
                        WindowEvent::Resized(x, y) => {
                            {
                                let mut settings = self.settings.borrow_mut();
                                settings.width = x as _;
                                settings.height = y as _;
                            }

                            self.graphical_pipeline
                                .mapshadow
                                .update(&self.settings.borrow());
                        }
                        _ => (),
                    }
                }

                Event::KeyDown {
                    timestamp,
                    window_id,
                    keycode,
                    scancode,
                    keymod,
                    repeat,
                } => {
                    if self.settings.borrow().debug {
                        println!(
                            "Event Key up: {} {} {:?} {:?} {} {}",
                            timestamp, window_id, keycode, scancode, keymod, repeat
                        );
                    }

                    if let Some(keycode) = keycode {
                        // Register selected key codes.
                        if [forward, left, backward, right, Keycode::LShift].contains(&keycode) {
                            let ok = !self.state.borrow().keys_down.contains(&keycode);
                            if ok {
                                self.state.borrow_mut().keys_down.push(keycode);
                            }
                        }

                        match keycode {
                            Keycode::M => {
                                let mode = self.scene.borrow_mut().camera.toggle_movement_mode();
                                match mode {
                                    MovementMode::Free => {
                                        self.win.sdl.mouse().set_relative_mouse_mode(true);
                                        self.center_cursor();
                                        let settings = self.settings.borrow();
                                        self.win
                                            .win
                                            .set_mouse_rect(Some(Rect::new(
                                                (settings.width / 2) as i32,
                                                (settings.height / 2) as i32,
                                                1,
                                                1,
                                            )))
                                            .unwrap();
                                    }
                                    _ => {
                                        self.win.sdl.mouse().set_relative_mouse_mode(false);
                                        self.win.win.set_mouse_rect(None).unwrap();
                                    }
                                };
                            }
                            Keycode::C => {
                                self.scene.borrow_mut().camera.target_anchor();
                            }
                            Keycode::P => {
                                self.toggle_pause();
                            }
                            Keycode::Q => {
                                self.export_quit();
                            }
                            Keycode::H => {
                                if (keymod & Mod::LSHIFTMOD) == Mod::LSHIFTMOD
                                    || (keymod & Mod::LSHIFTMOD) == Mod::RSHIFTMOD
                                {
                                    println!("Toggle debug!");
                                    self.toggle_debug();
                                } else {
                                    println!(
                                        "camera pos: {:?}",
                                        self.scene.borrow().camera.position.as_slice()
                                    );
                                    println!(
                                        "light pos: {:?}",
                                        self.scene.borrow().light.position.as_slice()
                                    );
                                }
                            }
                            Keycode::F10 => {
                                self.toggle_fullscreen_windowed();
                            }
                            Keycode::F11 => {
                                self.toggle_fullscreen();
                            }
                            _ => {}
                        }
                    }
                }

                Event::KeyUp {
                    timestamp: _,
                    window_id: _,
                    keycode,
                    scancode: _,
                    keymod: _,
                    repeat: _,
                } => {
                    // Unregister key codes.
                    if let Some(keycode) = keycode {
                        let opt_pos = self
                            .state
                            .borrow()
                            .keys_down
                            .iter()
                            .position(|x| *x == keycode);
                        if let Some(pos) = opt_pos {
                            self.state.borrow_mut().keys_down.remove(pos);
                        }
                    }
                }

                Event::MouseButtonDown {
                    timestamp,
                    window_id,
                    which,
                    mouse_btn,
                    clicks,
                    x,
                    y,
                } => {
                    if self.settings.borrow().debug {
                        println!(
                            "Event Button: {} {} {} {:?} {} {} {}",
                            timestamp, window_id, which, mouse_btn, clicks, x, y,
                        );
                    }

                    match mouse_btn {
                        MouseButton::Left => {
                            if self.win.sdl.mouse().is_cursor_showing() {
                                self.graphical_pipeline.picker.borrow_mut().pick(x, y);
                            }
                        }
                        _ => (),
                    }
                }

                Event::MouseMotion {
                    timestamp,
                    window_id,
                    which,
                    mousestate,
                    x,
                    y,
                    xrel,
                    yrel,
                } => {
                    if self.settings.borrow().debug {
                        println!(
                            "Event Motion: {} {} {} {:?} {} {} {} {}",
                            timestamp, window_id, which, mousestate, x, y, xrel, yrel,
                        );
                    }

                    let touchpad_controls = self.settings.borrow().touchpad_controls;
                    let speed = self.settings.borrow().sensitivity * clock_delta_time;
                    let mode = self.scene.borrow().camera.movement_mode;
                    match mode {
                        MovementMode::Free => {
                            self.scene
                                .borrow_mut()
                                .camera
                                .free_rotate(-xrel as Float * speed, -yrel as Float * speed);
                        }
                        MovementMode::Lock => {
                            if !touchpad_controls {
                                if mousestate.middle() {
                                    self.scene.borrow_mut().camera.lock_rotate(
                                        -xrel as Float * speed,
                                        -yrel as Float * speed,
                                    );
                                    self.warp_mouse_in_window_repeat(x, y);
                                }
                            }
                        }
                    }
                }

                Event::MouseWheel {
                    timestamp,
                    window_id,
                    which,
                    x,
                    y,
                    direction,
                    precise_x,
                    precise_y,
                    mouse_x: _mouse_x,
                    mouse_y: _mouse_y,
                } => {
                    if self.settings.borrow().debug {
                        println!(
                            "Event Wheel: {} {} {} {} {} {:?} {} {}",
                            timestamp, window_id, which, x, y, direction, precise_x, precise_y,
                        );
                    }

                    let touchpad_controls = self.settings.borrow().touchpad_controls;

                    if touchpad_controls {
                        let speed = self.settings.borrow().sensitivity
                            * SENSITIVITY_ROTATE_MOUSEWHEEL_CORRECTION
                            * clock_delta_time;
                        let mode = self.scene.borrow().camera.movement_mode;
                        match mode {
                            MovementMode::Free => {}
                            MovementMode::Lock => {
                                self.scene.borrow_mut().camera.lock_rotate(
                                    precise_x as Float * speed,
                                    -precise_y as Float * speed,
                                );
                            }
                        }
                    }
                }

                Event::Quit {
                    timestamp: _timestamp,
                } => return FrameEvent::Exit,
                thing => {
                    dbg!(thing);
                }
            }
        }

        // custom user keys for WASD / ZQSD

        let mode = self.scene.borrow().camera.movement_mode;
        match mode {
            MovementMode::Free => {
                let mut x = 0.0;
                let mut y = 0.0;
                let mut speed = SPEED;

                let keys = &self.state.borrow().keys_down;

                if keys.contains(&Keycode::LShift) {
                    speed *= SPEED_FAST_FACTOR;
                }
                let delta_pos = speed * clock_delta_time;

                if keys.contains(&forward) {
                    y += delta_pos;
                }
                if keys.contains(&left) {
                    x -= delta_pos;
                }
                if keys.contains(&backward) {
                    y -= delta_pos;
                }
                if keys.contains(&right) {
                    x += delta_pos;
                }

                self.scene.borrow_mut().camera.free_movement(x, y);
            }
            _ => {}
        }

        if self.state.borrow().quit {
            return FrameEvent::Exit;
        }

        FrameEvent::Continue
    }

    pub fn swap_window(&self) {
        self.win.win.gl_swap_window();
    }

    pub fn render_asteroids(&mut self, asteroids: &[AirlessBody]) {
        let width_viewport = self.settings.borrow().width_viewport();
        let height_viewport = self.settings.borrow().height_viewport();
        let aspect_ratio = self.settings.borrow().aspect_ratio();
        let shadows = self.settings.borrow().shadows;
        let directional_light_color = self.settings.borrow().directional_light_color;
        let ambient_light_color = self.settings.borrow().ambient_light_color;
        let draw_normals = self.settings.borrow().draw_normals;
        let wireframe = self.settings.borrow().wireframe;
        let wireframe_width = self.settings.borrow().wireframe_width;
        let diff_height = self
            .state
            .borrow()
            .difference_in_height_from_ratio_after_resize;

        let scene = self.scene.borrow();

        // Set constant uniforms for shader body.
        let matrix_projection = scene.camera.matrix_projection(aspect_ratio);

        let matrix_view = scene.camera.matrix_lookat();

        // let surfaces = asteroids.iter().map(|s| &s.surface).collect_vec();
        let matrices_model = asteroids.iter().map(|s| &s.matrix_model).collect_vec();

        self.clear_with_background();

        // println!("{} {} {}", width_viewport, height_viewport, diff_height);
        viewport_adaptative(width_viewport as u32, height_viewport as u32, diff_height);

        let shader = self.graphical_pipeline.shaders.body();
        shader.set_mat4("matrix_projection", &matrix_projection);
        shader.set_bool("shadows", shadows);
        shader.set_vec3("directional_light_color", &directional_light_color);
        shader.set_vec3("ambient_light_color", &ambient_light_color);
        shader.set_bool("force_color", false);
        shader.set_vec3("forced_color", &vec3(0.0, 0.0, 0.0));

        // Matrices for shader depth.
        let projection_light_matrix = scene.light.matrix_projection(aspect_ratio);
        let matrix_view_light = scene.light.matrix_lookat();
        let matrix_lightspace = projection_light_matrix * matrix_view_light;

        self.render_depth_to_texture(&scene, &matrix_lightspace, &matrices_model);
        self.render_scene(
            &scene,
            &matrix_projection,
            &matrix_view,
            &matrices_model,
            &matrix_lightspace,
            wireframe,
            wireframe_width,
        );

        if draw_normals {
            self.render_normals(&scene, &matrix_view, &matrix_projection, &matrices_model);
        }

        self.debug_depth_map();

        self.mouse_click(asteroids, &scene, &matrix_projection, &matrix_view);

        self.render_trajectories(&matrix_projection, &matrix_view, &scene)
    }

    pub fn render_trajectories(
        &self,
        matrix_projection: &Mat4,
        matrix_view: &Mat4,
        scene: &WindowScene,
    ) {
        let shader = self.graphical_pipeline.shaders.trajectory();
        shader.set_mat4("matrix_projection", &matrix_projection);
        shader.set_mat4("matrix_view", &matrix_view);
        shader.set_mat4("matrix_model", &Mat4::identity());
        shader.set_vec3("color", &vec3(0.0, 0.0, 0.0));

        for vao in &scene.trajectories_vao {
            shader.draw(vao, gl::LINE_STRIP);
        }
    }

    fn render_depth_to_texture(
        &self,
        scene: &WindowScene,
        matrix_lightspace: &Mat4,
        matrices_model: &[&Mat4],
    ) {
        let shadows = self.settings.borrow().shadows;
        if !shadows {
            return;
        }

        let front_face_culling_for_peter_panning =
            self.settings.borrow().front_face_culling_for_peter_panning;

        unsafe {
            gl::BindFramebuffer(
                gl::FRAMEBUFFER,
                self.graphical_pipeline.mapshadow.framebuffer,
            );
        }

        self.clear(gl::DEPTH_BUFFER_BIT);

        let shader = self.graphical_pipeline.shaders.depth();
        shader.set_mat4("matrix_lightspace", &matrix_lightspace);

        if front_face_culling_for_peter_panning {
            unsafe {
                gl::CullFace(gl::FRONT);
            }
        }

        self.draw_user_rendering(scene, shader, matrices_model, false, 1.0);

        if front_face_culling_for_peter_panning {
            unsafe {
                gl::CullFace(gl::BACK);
            }
        }

        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        }
    }

    fn render_scene(
        &self,
        scene: &WindowScene,
        matrix_projection: &Mat4,
        matrix_view: &Mat4,
        matrices_model: &[&Mat4],
        matrix_lightspace: &Mat4,
        wireframe: bool,
        wireframe_width: Float,
    ) {
        let bias_acne = self.settings.borrow().bias_acne;
        let show_light = self.settings.borrow().show_light;
        let colormap = self.settings.borrow().colormap;
        let colormap_bounds = self.settings.borrow().colormap_bounds;

        self.clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);

        // Matrices for shader body.
        let matrix_model_light = Mat4::new_translation(&scene.light.position);
        let matrix_model_light = glm::scale(&matrix_model_light, &Vec3::from_element(0.1));
        let matrix_mvp_light = matrix_projection * matrix_view * matrix_model_light;

        // Set uniforms for shader body.
        let shader = self.graphical_pipeline.shaders.body();
        shader.set_mat4("matrix_lightspace", &matrix_lightspace);
        shader.set_mat4("matrix_view", &matrix_view);

        shader.set_vec3("pos_light", &scene.light.position);
        shader.set_vec3("pos_camera", &scene.camera.position);
        shader.set_f("bias_acne", bias_acne);

        shader.set_i("colormap_code", colormap as isize);

        let bounds = colormap_bounds;
        shader.set_vec2("colormap_bounds", &vec2(bounds.0, bounds.1));

        unsafe {
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.graphical_pipeline.mapshadow.map_depth);
        }

        if self.settings.borrow().debug_depth_map {
            return;
        }

        self.draw_user_rendering(scene, shader, matrices_model, wireframe, wireframe_width);

        // Set uniforms for shader light.
        if show_light {
            let shader = self.graphical_pipeline.shaders.light();
            shader.set_mat4("matrix_model_view_projection", &matrix_mvp_light);
            shader.draw(scene.light_vao.as_ref().unwrap(), gl::TRIANGLES);
        }
    }

    fn render_normals(
        &self,
        scene: &WindowScene,
        matrix_view: &Mat4,
        matrix_projection: &Mat4,
        matrices_model: &[&Mat4],
    ) {
        let normals_magnitude = self.settings.borrow().normals_magnitude;

        let shader = self.graphical_pipeline.shaders.normals();
        shader.set_mat4("matrix_projection", &matrix_projection);
        shader.set_mat4("matrix_view", &matrix_view);
        shader.set_f("magnitude", normals_magnitude);

        for (&matrix_model, vao) in izip!(matrices_model, &scene.bodies_vao) {
            let matrix_mv = matrix_view * matrix_model;
            let matrix_normal_mv = glm::transpose(&glm::inverse(&matrix_mv))
                .fixed_view::<3, 3>(0, 0)
                .into();
            shader.set_mat4("matrix_model", matrix_model);
            shader.set_mat3("matrix_normal_MV", &matrix_normal_mv);
            shader.draw(vao, gl::TRIANGLES);
        }
    }

    fn debug_depth_map(&self) {
        if !self.settings.borrow().debug_depth_map {
            return;
        }

        let _shader = self.graphical_pipeline.shaders.debug_depth();

        unsafe {
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.graphical_pipeline.mapshadow.map_depth);
        }

        self.graphical_pipeline.vao_debug_depth.draw(gl::TRIANGLES);
    }

    fn draw_user_rendering(
        &self,
        scene: &WindowScene,
        shader: &Shader,
        matrices_model: &[&Mat4],
        wireframe: bool,
        wireframe_width: Float,
    ) {
        for (matrix_model, vao) in izip!(matrices_model, &scene.bodies_vao) {
            let matrix_normal = glm::transpose(&glm::inverse(matrix_model))
                .fixed_view::<3, 3>(0, 0)
                .into();
            shader.set_mat4("matrix_model", matrix_model);
            shader.set_mat3("matrix_normal", &matrix_normal);

            if wireframe {
                // params for draw inside.
                unsafe {
                    gl::PolygonOffset(1.0, 1.0);
                    gl::PolygonMode(gl::FRONT_AND_BACK, gl::FILL);
                    gl::Enable(gl::POLYGON_OFFSET_FILL);
                }
                shader.draw(vao, gl::TRIANGLES);

                // params for draw wireframe.
                shader.set_bool("force_color", true);
                unsafe {
                    gl::LineWidth(wireframe_width as f32);
                    gl::PolygonMode(gl::FRONT_AND_BACK, gl::LINE);
                    gl::Disable(gl::POLYGON_OFFSET_FILL);
                }
                shader.draw(vao, gl::TRIANGLES);

                // reset.
                shader.set_bool("force_false", false);
                unsafe {
                    gl::LineWidth(1.0);
                }
            } else {
                shader.draw(vao, gl::TRIANGLES);
            }
        }
    }

    pub fn export_frame<P: AsRef<Path>>(&self, path: P) {
        let width = self.settings.borrow().width_viewport();
        let height = self.settings.borrow().height_viewport();

        let mut img = image::RgbImage::new(width as u32, height as u32);

        unsafe {
            gl::PixelStorei(gl::PACK_ALIGNMENT, 4);
            gl::ReadBuffer(gl::FRONT);
            gl::ReadPixels(
                0,
                0,
                width as _,
                height as _,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                img.as_mut_ptr() as _,
            );
        }

        image::imageops::flip_vertical_in_place(&mut img);

        let path = path.as_ref();
        fs::create_dir_all(&path.parent().unwrap()).unwrap();

        img.save(path).unwrap();
    }

    fn mouse_click(
        &self,
        asteroids: &[AirlessBody],
        scene: &WindowScene,
        matrix_projection: &Mat4,
        matrix_view: &Mat4,
    ) {
        let mut picker = self.graphical_pipeline.picker.borrow_mut();

        if let Some((ndc_x, ndc_y)) = picker.use_pick_to_ndc(&self.settings.borrow()) {
            let (raystart_world, raydir_world) =
                if scene.borrow().camera.projection == ProjectionMode::Orthographic {
                    let raystart_world = -(glm::inverse(matrix_view)
                        * glm::inverse(matrix_projection)
                        * vec4(-ndc_x, ndc_y, 0.0, 1.0))
                    .xyz();

                    let raydir_world = scene.camera.front();

                    (raystart_world, raydir_world)
                } else {
                    let start_view = vec3(0.0, 0.0, 0.0);

                    let dir_view = vec4(ndc_x, -ndc_y, 0.0, 1.0);
                    let dir_view = glm::inverse(&matrix_projection) * dir_view;
                    let dir_view = dir_view.xyz().normalize();

                    let start_world = glm::inverse(&matrix_view) * vec3_to_4_one(&start_view);
                    let start_world = start_world.xyz();

                    let end_world =
                        glm::inverse(&matrix_view) * vec3_to_4_one(&(start_view + dir_view));
                    let end_world = end_world.xyz();

                    let dir_world = (end_world - start_world).normalize();

                    (start_world, dir_world)
                };

            /*
            println!();
            println!("{} {}", ndc_x, ndc_y);
            println!("raystart world: {:?}", raystart_world.as_slice());
            println!("raydir world: {:?}", raydir_world.as_slice());
            println!("cam pos: {:?}", scene.camera.position.as_slice());
            */

            let maybe_intersect = intersect_asteroids(&raystart_world, &raydir_world, asteroids);

            if let Some((intersect, face_index, surface_index)) = maybe_intersect {
                println!("Clicked on surface #{} face #{}", surface_index, face_index);
                println!("{}", asteroids[surface_index].surface.faces[face_index]);
                println!("Intersection point: {:?}", intersect.as_slice());
                println!();

                picker.picked_on = Some((face_index, surface_index));
            } else {
                picker.picked_on = None;
            }
        }
    }

    pub fn render_gui<R>(&mut self, add_contents: impl FnOnce(&mut Ui) -> R) {
        unsafe {
            // Clear the screen to green
            gl::ClearColor(0.3, 0.6, 0.3, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        let elapsed = self.clock.elapsed();
        let gui = &mut self.win.egui;
        gui.state.input.time = Some(elapsed);
        gui.ctx.begin_frame(gui.state.input.take());

        egui::CentralPanel::default().show(&gui.ctx, |ui| {
            add_contents(ui);
        });

        let FullOutput {
            platform_output,
            textures_delta,
            shapes,
            pixels_per_point,
            viewport_output,
        } = gui.ctx.end_frame();

        gui.state.process_output(&self.win.win, &platform_output);

        let paint_jobs = gui.ctx.tessellate(shapes, pixels_per_point);
        gui.painter.paint_jobs(None, textures_delta, paint_jobs);

        gui.viewport_output = viewport_output;
    }

    pub fn render_gui_finish(&mut self) {
        // let gui = &mut self.win.egui;
        // let repaint_after = gui
        //     .viewport_output
        //     .get(&egui::ViewportId::ROOT)
        //     .expect("Missing ViewportId::ROOT")
        //     .repaint_delay;

        // let quit = &mut self.state.borrow_mut().quit;
        // let mut event_pump = self.win.sdl.event_pump().unwrap();

        /*
        if !repaint_after.is_zero() {
            if let Some(event) = event_pump.wait_event_timeout(5) {
                match event {
                    Event::Quit { .. } => *quit = true,
                    _ => {
                        gui.state
                            .process_input(&self.win.win, event, &mut gui.painter);
                    }
                }
            }
        } else {
            for event in event_pump.poll_iter() {
                match event {
                    Event::Quit { .. } => *quit = true,
                    _ => {
                        gui.state
                            .process_input(&self.win.win, event, &mut gui.painter);
                    }
                }
            }
        }
        */
    }
}
