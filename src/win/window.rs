use crate::prelude::*;
// use crate::python::*;

use crate::ast::ray;
use crate::win::camera::*;
use crate::win::graphical_pipeline::*;
use crate::win::scene::Scene;
use crate::win::window_settings::*;

// use beryllium::events::*;
// use beryllium::init::*;
// use beryllium::video::*;
// use beryllium::Sdl;
// use fermium::prelude::*;

// use gl33::global_loader::*;
// use gl33::*;

use sdl2;
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::keyboard::Mod;
use sdl2::mouse::MouseButton;
use sdl2::sys as sdl2_sys;
use sdl2::video::GLContext;
use sdl2::video::Window as SDL_Window;
use sdl2::video::{GLProfile, SwapInterval};
use sdl2::Sdl;

use gl::types::*;

use std::cell::RefCell;
use std::fmt::Debug;

use std::fs;
use std::path::Path;

use std::ffi::CStr;

#[allow(unused)]
fn viewport(width: u32, height: u32) {
    unsafe { gl::Viewport(0, 0, width as _, height as _) }
}

fn viewport_adaptative(width: u32, height: u32, diff_height: i32) {
    unsafe { gl::Viewport(0, diff_height as _, width as _, (height as i32) as _) }
}

/*
fn viewport_center(width: u32, height: u32) {
    unsafe {
        glViewport(
            -(width as f32 / 2.0) as _,
            -(height as f32 / 2.0) as _,
            (width as f32 / 2.0) as _,
            (height as f32 / 2.0) as _,
        )
    }
}
*/

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameEvent {
    Continue,
    Exit,
    None,
}

#[derive(Debug)]
pub struct Clock {
    pub now: RefCell<u64>,
}

impl Clock {
    pub fn new() -> Self {
        Self {
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
}

pub struct RawWindow {
    pub sdl: Sdl,
    pub win: SDL_Window,
    pub ctx: GLContext,
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

        video.gl_set_swap_interval(SwapInterval::VSync).unwrap();

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

        Self { sdl, win, ctx }
    }
}

#[derive(Debug)]
pub struct Window {
    pub win: RawWindow,
    pub settings: RefCell<WindowSettings>,
    pub state: RefCell<WindowState>,
    pub clock: Clock,
    pub graphical_pipeline: GraphicalPipeline,
    pub scene: RefCell<Scene>,
}

impl Window {
    pub fn new(settings: WindowSettings) -> Self {
        let win = RawWindow::new(&settings);
        let state = WindowState::default();

        let graphical_pipeline = GraphicalPipeline::new(&settings);
        let clock = Clock::new();
        let scene = Scene::new(&settings);

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

    pub fn with_light_position(self, pos: &Vec3) -> Self {
        {
            let mut scene = self.scene.borrow_mut();
            scene.set_light_position(pos);
        }
        self
    }

    pub fn with_light_direction(self, dir: &Vec3) -> Self {
        {
            let mut scene = self.scene.borrow_mut();
            scene.set_light_direction(dir);
        }
        self
    }

    pub fn with_camera_position(self, pos: &Vec3) -> Self {
        {
            let mut scene = self.scene.borrow_mut();
            scene.set_camera_position(pos);
        }
        self
    }

    pub fn camera_position(&self) -> Vec3 {
        self.scene.borrow().camera_position().clone()
    }

    pub fn set_light_position(&self, pos: &Vec3) {
        let mut scene = self.scene.borrow_mut();
        scene.set_light_position(pos);
    }

    pub fn set_light_direction(&self, dir: &Vec3) {
        let mut scene = self.scene.borrow_mut();
        scene.set_light_direction(dir);
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

    pub fn events(&mut self) -> FrameEvent {
        let clock_delta_time = self.clock.delta_time();
        let mut event_pump = self.win.sdl.event_pump().unwrap();

        for event in event_pump.poll_iter() {
            match event {
                Event::Window {
                    timestamp,
                    window_id,
                    win_event,
                } => {
                    if self.settings.borrow().debug {
                        println!("{} {} {:?}", timestamp, window_id, win_event);
                    }

                    match win_event {
                        WindowEvent::Resized(x, y) => {
                            {
                                let mut settings = self.settings.borrow_mut();
                                settings.width = x as _;
                                settings.height = y as _;
                                /*
                                let mut state = self.state.borrow_mut();
                                settings.height = (x as Float / WINDOW_RATIO) as _;
                                state.difference_in_height_from_ratio_after_resize =
                                    y - settings.height as i32;
                                */
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
                            "{} {} {:?} {:?} {} {}",
                            timestamp, window_id, keycode, scancode, keymod, repeat
                        );
                    }

                    if let Some(keycode) = keycode {
                        match (keycode, keymod) {
                            (Keycode::Up | Keycode::Down, Mod::LSHIFTMOD) => {
                                self.scene.borrow_mut().camera.change_speed(
                                    Direction::from_keycode(keycode),
                                    clock_delta_time,
                                );
                            }
                            (
                                Keycode::Up | Keycode::Left | Keycode::Down | Keycode::Right,
                                _mod,
                            ) => self
                                .scene
                                .borrow_mut()
                                .camera
                                .move_command(Direction::from_keycode(keycode), clock_delta_time),
                            (Keycode::M, _) => {
                                self.scene.borrow_mut().camera.change_move_method();
                            }
                            (Keycode::C, _) => {
                                self.scene.borrow_mut().camera.target_origin();
                            }
                            (Keycode::P, _) => {
                                self.toggle_pause();
                            }
                            (Keycode::Q, _) => {
                                self.export_quit();
                            }
                            // (Keycode::Comma, Mod::LSHIFTMOD | Mod::RSHIFTMOD | Mod::CAPSMOD) => {
                            (Keycode::H, mode) => {
                                if (mode & Mod::LSHIFTMOD) == Mod::LSHIFTMOD
                                    || (mode & Mod::LSHIFTMOD) == Mod::RSHIFTMOD
                                {
                                    println!("Toggle debug!");
                                    self.toggle_debug();
                                } else {
                                    println!("-- Debug Scene --");
                                    println!("camera pos: {:?}", self.scene.borrow().camera.position.as_slice());
                                    println!("light pos: {:?}", self.scene.borrow().light.position.as_slice());
                                    println!("-----------------");
                                }
                            }
                            (Keycode::F10, _) => {
                                self.toggle_fullscreen_windowed();
                            }
                            (Keycode::F11, _) => {
                                self.toggle_fullscreen();
                            }
                            (_, _) => {}
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
                            "{} {} {} {:?} {} {} {}",
                            timestamp, window_id, which, mouse_btn, clicks, x, y,
                        );
                    }

                    match mouse_btn {
                        MouseButton::Left => {
                            let mut picker = self.graphical_pipeline.picker.borrow_mut();

                            // let height = self.settings.borrow().height;
                            // let y = height as i32 - y - 1;

                            picker.pick(x, y);
                        }
                        _ => (),
                    }
                }

                Event::Quit {
                    timestamp: _timestamp,
                } => return FrameEvent::Exit,
                _ => (),
            }
        }
        FrameEvent::Continue
    }

    pub fn swap_window(&self) {
        self.win.win.gl_swap_window();
    }

    pub fn render_asteroids(&mut self, asteroids: &[&Asteroid]) {
        let width_viewport = self.settings.borrow().width_viewport();
        let height_viewport = self.settings.borrow().height_viewport();
        let aspect_ratio = self.settings.borrow().aspect_ratio();
        let shadows = self.settings.borrow().shadows;
        let directional_light_color = self.settings.borrow().directional_light_color;
        let ambient_light_color = self.settings.borrow().ambient_light_color;
        let ortho = self.settings.borrow().ortho;
        let fov = self.settings.borrow().fov;
        let far_factor = self.settings.borrow().far_factor;
        let close_distance = self.settings.borrow().close_distance;
        let draw_normals = self.settings.borrow().draw_normals;
        let wireframe = self.settings.borrow().wireframe;
        let wireframe_width = self.settings.borrow().wireframe_width;
        let diff_height = self
            .state
            .borrow()
            .difference_in_height_from_ratio_after_resize;

        let scene = self.scene.borrow_mut();

        // Set constant uniforms for shader body.
        let distance = scene.camera.position.magnitude();
        let close = close_distance;
        let far = distance * 2.0 * far_factor;
        let matrix_projection = if ortho {
            let side = distance;
            glm::ortho(
                -side * aspect_ratio,
                side * aspect_ratio,
                -side,
                side,
                close,
                far,
            )
        } else {
            glm::perspective(aspect_ratio, fov * RPD, close, far)
        };

        let matrix_view = scene.camera.look_at();

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
        let projection_light_matrix = scene.light.projection.matrix;
        let matrix_view_light = glm::look_at(
            &scene.light.position,
            &vec3(0.0, 0.0, 0.0),
            &vec3(0.0, 0.0, 1.0),
        );
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

    pub fn render_trajectories(&self, matrix_projection: &Mat4, matrix_view: &Mat4, scene: &Scene) {
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
        scene: &Scene,
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
        scene: &Scene,
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
        scene: &Scene,
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
        scene: &Scene,
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
        asteroids: &[&Asteroid],
        scene: &Scene,
        matrix_projection: &Mat4,
        matrix_view: &Mat4,
    ) {
        let mut picker = self.graphical_pipeline.picker.borrow_mut();
        let ortho = self.settings.borrow().ortho;

        if let Some((ndc_x, ndc_y)) = picker.use_pick_to_ndc(&self.settings.borrow()) {
            let (raystart_world, raydir_world) = if ortho {
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

                let start_world = glm::inverse(&matrix_view) * util::vec3_to_4_one(&start_view);
                let start_world = start_world.xyz();

                let end_world =
                    glm::inverse(&matrix_view) * util::vec3_to_4_one(&(start_view + dir_view));
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

            let maybe_intersect =
                ray::intersect_asteroids(&raystart_world, &raydir_world, asteroids);

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
}
