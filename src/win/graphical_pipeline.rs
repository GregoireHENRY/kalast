use crate::util::*;
use crate::{Material, Vertex, WindowSettings};

use gl::types::*;
use itertools::Itertools;

use std::cell::RefCell;
use std::convert::TryInto;
use std::ffi::CString;
use std::path::Path;
use std::{fs, ptr, str};

#[allow(unused)]
use std::mem::{size_of, size_of_val};

pub const VS_DEPTH: &str = include_str!("../../shaders/depth.vs");
pub const FS_DEPTH: &str = include_str!("../../shaders/depth.fs");
pub const VS_BODY: &str = include_str!("../../shaders/body.vs");
pub const FS_BODY: &str = include_str!("../../shaders/body.fs");
pub const VS_LIGHT: &str = include_str!("../../shaders/light.vs");
pub const FS_LIGHT: &str = include_str!("../../shaders/light.fs");
pub const VS_DEBUG_DEPTH: &str = include_str!("../../shaders/debug_depth.vs");
pub const FS_DEBUG_DEPTH: &str = include_str!("../../shaders/debug_depth.fs");
pub const VS_PICKING: &str = include_str!("../../shaders/picking.vs");
pub const FS_PICKING: &str = include_str!("../../shaders/picking.fs");
pub const VS_NORMALS: &str = include_str!("../../shaders/normals.vs");
pub const FS_NORMALS: &str = include_str!("../../shaders/normals.fs");
pub const GS_NORMALS: &str = include_str!("../../shaders/normals.gs");
pub const VS_TRAJECTORY: &str = include_str!("../../shaders/trajectory.vs");
pub const FS_TRAJECTORY: &str = include_str!("../../shaders/trajectory.fs");

pub fn create_program(vs: u32, fs: u32, gs: Option<u32>) -> u32 {
    let program = unsafe {
        let program = gl::CreateProgram();

        gl::AttachShader(program, vs);
        gl::AttachShader(program, fs);

        if let Some(gs) = gs {
            gl::AttachShader(program, gs);
        }

        gl::LinkProgram(program);

        gl::DeleteShader(vs);
        gl::DeleteShader(fs);

        if let Some(gs) = gs {
            gl::DeleteShader(gs);
        }

        program
    };

    check_shader(program, gl::PROGRAM);

    program
}

pub fn load_shader_from_file<P: AsRef<Path>>(source: P, kind: GLenum) -> u32 {
    let buf = fs::read(source.as_ref()).unwrap();
    let source_str = str::from_utf8(&buf).unwrap();

    load_shader(source_str, kind)
}

pub fn load_shader(source: &str, kind: GLenum) -> u32 {
    match kind {
        gl::VERTEX_SHADER | gl::FRAGMENT_SHADER | gl::GEOMETRY_SHADER => (),
        _ => panic!("Incorrect kind of shader that you try to load."),
    }

    let shader = unsafe {
        let shader = gl::CreateShader(kind);
        assert_ne!(shader, 0);

        gl::ShaderSource(
            shader,
            1,
            &(source.as_bytes().as_ptr().cast()),
            &(source.len().try_into().unwrap()),
        );

        gl::CompileShader(shader);

        shader
    };

    check_shader(shader, kind);

    shader
}

pub fn check_shader(id: u32, kind: GLenum) {
    // Check for error.
    let mut success = 0;
    let mut v = Vec::<u8>::with_capacity(1024);
    let mut log_len = 0_i32;

    match kind {
        gl::VERTEX_SHADER | gl::FRAGMENT_SHADER | gl::GEOMETRY_SHADER => unsafe {
            gl::GetShaderiv(id, gl::COMPILE_STATUS, &mut success)
        },
        gl::PROGRAM => unsafe { gl::GetProgramiv(id, gl::LINK_STATUS, &mut success) },
        _ => panic!("Incorrect kind of shader/program that you try to check."),
    };

    if success == 0 {
        unsafe {
            gl::GetShaderInfoLog(id, 1024, &mut log_len, v.as_mut_ptr().cast());
            v.set_len(log_len.try_into().unwrap());
        }

        let error_desc = match kind {
            gl::VERTEX_SHADER => "Vertex shader Compilation Failed",
            gl::FRAGMENT_SHADER => "Fragment shader Compilation Failed",
            gl::GEOMETRY_SHADER => "Geometry shader Compilation Failed",
            gl::PROGRAM => "Program Linking Failed",
            _ => unreachable!("should have panicked before in shader::check_shader()"),
        };

        panic!("Error {}: {}", error_desc, String::from_utf8_lossy(&v));
    }
}

#[derive(Debug)]
pub struct Shader {
    pub id: u32,
}

impl Shader {
    pub fn from_path<P: AsRef<Path>>(path_vs: P, path_fs: P, path_gs: Option<P>) -> Self {
        let vs = load_shader_from_file(path_vs, gl::VERTEX_SHADER);
        let fs = load_shader_from_file(path_fs, gl::FRAGMENT_SHADER);
        let gs = path_gs.and_then(|p| Some(load_shader_from_file(p, gl::GEOMETRY_SHADER)));
        let id = create_program(vs, fs, gs);

        Self { id }
    }

    pub fn from_included(str_vs: &str, str_fs: &str, str_gs: Option<&str>) -> Self {
        let vs = load_shader(str_vs, gl::VERTEX_SHADER);
        let fs = load_shader(str_fs, gl::FRAGMENT_SHADER);
        let gs = str_gs.and_then(|s| Some(load_shader(s, gl::GEOMETRY_SHADER)));
        let id = create_program(vs, fs, gs);

        Self { id }
    }

    pub fn location(&self, var: &str) -> i32 {
        self.select();
        let var = CString::new(var).unwrap();
        unsafe { gl::GetUniformLocation(self.id, var.as_ptr() as *const _) }
    }

    pub fn loc(&self, var: &str) -> i32 {
        self.location(var)
    }

    pub fn set_vec4(&self, var: &str, v: &Vec4) {
        let v: glm::Vec4 = glm::convert(*v);
        unsafe {
            gl::Uniform4f(self.loc(var), v.x, v.y, v.z, v.w);
        }
    }

    pub fn set_vec3(&self, var: &str, v: &Vec3) {
        let v: glm::Vec3 = glm::convert(*v);
        unsafe {
            gl::Uniform3f(self.loc(var), v.x, v.y, v.z);
        }
    }

    pub fn set_vec2(&self, var: &str, v: &Vec2) {
        let v: glm::Vec2 = glm::convert(*v);
        unsafe {
            gl::Uniform2f(self.loc(var), v.x, v.y);
        }
    }

    pub fn set_i(&self, var: &str, x: isize) {
        unsafe {
            gl::Uniform1i(self.loc(var), x as i32);
        }
    }

    pub fn set_u(&self, var: &str, x: usize) {
        unsafe {
            gl::Uniform1ui(self.loc(var), x as u32);
        }
    }

    pub fn set_f(&self, var: &str, x: Float) {
        unsafe {
            gl::Uniform1f(self.loc(var), x as f32);
        }
    }

    pub fn set_bool(&self, var: &str, x: bool) {
        self.set_i(var, x as _);
    }

    pub fn set_mat4(&self, var: &str, m: &Mat4) {
        let m: glm::Mat4 = glm::convert(*m);
        unsafe { gl::UniformMatrix4fv(self.loc(var), 1, 0, glm::value_ptr(&m).as_ptr()) }
    }

    pub fn set_mat3(&self, var: &str, m: &Mat3) {
        let m: glm::Mat3 = glm::convert(*m);
        unsafe { gl::UniformMatrix3fv(self.loc(var), 1, 0, glm::value_ptr(&m).as_ptr()) }
    }

    /*
    pub fn create_uniform_buffer<const N: usize>(&self, buffers: [(&str, Vec<&str>, usize); N]) {
        let mut ubos: HashMap<String, (u32, u32)> = HashMap::new();

        for (index_binding, (block, programs, size)) in buffers.iter().enumerate() {
            let var = CString::new(block.to_string()).unwrap();
            for &program in programs.iter() {
                unsafe {
                    let index_block =
                        gl::GetUniformBlockIndex(self.programs[program], var.as_ptr() as *const _);
                    gl::UniformBlockBinding(
                        self.programs[program],
                        index_block,
                        index_binding as u32,
                    );
                }
            }

            let mut ubo: u32 = 0;
            unsafe {
                gl::GenBuffers(1, &mut ubo);
                gl::BindBuffer(gl::UNIFORM_BUFFER, ubo);
                gl::BufferData(
                    gl::UNIFORM_BUFFER,
                    *size as isize,
                    ptr::null() as *const _,
                    gl::DYNAMIC_DRAW,
                );
                gl::BindBufferBase(gl::UNIFORM_BUFFER, index_binding as u32, ubo);
            }

            ubos.insert(block.to_string(), (ubo, index_binding as u32));
        }

        *self.ubos.borrow_mut() = ubos;
    }

    pub fn set_uniform_buffer_mat4f<const N: usize>(&self, vars: [&Mat4; N], block: &str) {
        let ubo = self.ubos.borrow()[block].0;
        unsafe { gl::BindBuffer(gl::UNIFORM_BUFFER, ubo) };

        let size_var = size_of::<Mat4>();
        let size_var_aligned = size_of::<Mat4>();

        for (index, var) in vars.iter().enumerate() {
            unsafe {
                gl::BufferSubData(
                    gl::UNIFORM_BUFFER,
                    (index * size_var_aligned) as isize,
                    size_var as isize,
                    glm::value_ptr(&var).as_ptr() as *const _,
                );
            }
        }
    }

    pub fn set_uniform_buffer_vec3f<const N: usize>(&self, vars: [&Vec3; N], block: &str) {
        let ubo = self.ubos.borrow()[block].0;
        unsafe { gl::BindBuffer(gl::UNIFORM_BUFFER, ubo) };

        let size_var = size_of::<Vec3>();
        let size_var_aligned = size_of::<Vec4>();

        for (index, var) in vars.iter().enumerate() {
            unsafe {
                gl::BufferSubData(
                    gl::UNIFORM_BUFFER,
                    (index * size_var_aligned) as isize,
                    size_var as isize,
                    glm::value_ptr(&var).as_ptr() as *const _,
                );
            }
        }
    }
    */

    pub fn select(&self) {
        unsafe {
            gl::UseProgram(self.id);
        }
    }

    pub fn draw(&self, vao: &VAO, mode: GLenum) {
        self.select();
        vao.draw(mode);
    }
}

#[derive(Debug)]
pub struct Shaders {
    pub depth: Shader,
    pub body: Shader,
    pub light: Shader,
    pub debug_depth: Shader,
    pub picking: Shader,
    pub normals: Shader,
    pub trajectory: Shader,
}

impl Shaders {
    pub fn new() -> Self {
        let depth = Shader::from_included(VS_DEPTH, FS_DEPTH, None);
        let body = Shader::from_included(VS_BODY, FS_BODY, None);
        let light = Shader::from_included(VS_LIGHT, FS_LIGHT, None);
        let debug_depth = Shader::from_included(VS_DEBUG_DEPTH, FS_DEBUG_DEPTH, None);
        let picking = Shader::from_included(VS_PICKING, FS_PICKING, None);
        let normals = Shader::from_included(VS_NORMALS, FS_NORMALS, Some(GS_NORMALS));
        let trajectory = Shader::from_included(VS_TRAJECTORY, FS_TRAJECTORY, None);

        Self {
            depth,
            body,
            light,
            debug_depth,
            picking,
            normals,
            trajectory,
        }
    }

    pub fn depth(&self) -> &Shader {
        self.depth.select();
        &self.depth
    }

    pub fn body(&self) -> &Shader {
        self.body.select();
        &self.body
    }

    pub fn light(&self) -> &Shader {
        self.light.select();
        &self.light
    }

    pub fn debug_depth(&self) -> &Shader {
        self.debug_depth.select();
        &self.debug_depth
    }

    pub fn picking(&self) -> &Shader {
        self.picking.select();
        &self.picking
    }

    pub fn normals(&self) -> &Shader {
        self.normals.select();
        &self.normals
    }

    pub fn trajectory(&self) -> &Shader {
        self.trajectory.select();
        &self.trajectory
    }
}

#[derive(Debug, Clone)]
pub struct VAO {
    pub vao: u32,
    vbo: u32,
    ebo: u32,
    len: usize,
}

impl VAO {
    /*
    fn new() -> Self {
        let mut vao = 0;
        let mut vbo = 0;
        let mut ebo = 0;

        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
            gl::GenBuffers(1, &mut ebo);
        }

        Self {
            vao,
            vbo,
            ebo,
            len: 0,
        }
    }
    */

    pub fn smooth_element_buffers(vertices: &[Vertex], indices: &[u32]) -> Self {
        let mut vao = 0;
        let mut vbo = 0;
        let mut ebo = 0;

        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
            gl::GenBuffers(1, &mut ebo);
        }

        let size_vertex_data = size_of::<Vertex>();
        let len = indices.len();

        unsafe {
            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (vertices.len() * size_vertex_data) as _,
                vertices.as_ptr() as _,
                gl::STATIC_DRAW,
            );

            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, ebo);
            gl::BufferData(
                gl::ELEMENT_ARRAY_BUFFER,
                (len * size_of::<u32>()) as _,
                indices.as_ptr() as _,
                gl::STATIC_DRAW,
            );

            // Vertex position
            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(
                0,
                3,
                gl::DOUBLE,
                0,
                size_vertex_data as _,
                (0 * size_of::<Vec3>()) as _,
            );

            // Vertex normal
            gl::EnableVertexAttribArray(1);
            gl::VertexAttribPointer(
                1,
                3,
                gl::DOUBLE,
                0,
                size_vertex_data as _,
                (1 * size_of::<Vec3>()) as _,
            );

            // Vertex color
            gl::EnableVertexAttribArray(2);
            gl::VertexAttribPointer(
                2,
                3,
                gl::DOUBLE,
                0,
                size_vertex_data as _,
                (2 * size_of::<Vec3>()) as _,
            );

            // Data
            gl::EnableVertexAttribArray(3);
            gl::VertexAttribPointer(
                3,
                1,
                gl::DOUBLE,
                0,
                size_vertex_data as _,
                (3 * size_of::<Vec3>()) as _,
            );

            // Albedo
            gl::EnableVertexAttribArray(4);
            gl::VertexAttribPointer(
                4,
                1,
                gl::DOUBLE,
                0,
                size_vertex_data as _,
                (3 * size_of::<Vec3>() + 1 * size_of::<Float>()) as _,
            );

            // Color Mode
            gl::EnableVertexAttribArray(5);
            gl::VertexAttribIPointer(
                5,
                1,
                gl::INT,
                size_vertex_data as _,
                (3 * size_of::<Vec3>() + 1 * size_of::<Float>() + 1 * size_of::<Material>()) as _,
            );

            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, 0);
        }

        Self { vao, vbo, ebo, len }
    }

    pub fn flat_vertices_buffers(vertices: &[Vertex]) -> Self {
        let mut vao = 0;
        let mut vbo = 0;

        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
        }

        let size_vertex_data = size_of::<Vertex>();
        let len = vertices.len();

        // println!("{} {}: {} {}", vao, vbo, size_vertex_data, len);

        unsafe {
            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (len * size_vertex_data) as _,
                vertices.as_ptr() as _,
                gl::STATIC_DRAW,
            );

            // Vertex position
            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(
                0,
                3,
                gl::DOUBLE,
                0,
                size_vertex_data as _,
                (0 * size_of::<Vec3>()) as _,
            );

            // Vertex normal
            gl::EnableVertexAttribArray(1);
            gl::VertexAttribPointer(
                1,
                3,
                gl::DOUBLE,
                0,
                size_vertex_data as _,
                (1 * size_of::<Vec3>()) as _,
            );

            // Vertex color
            gl::EnableVertexAttribArray(2);
            gl::VertexAttribPointer(
                2,
                3,
                gl::DOUBLE,
                0,
                size_vertex_data as _,
                (2 * size_of::<Vec3>()) as _,
            );

            // Data
            gl::EnableVertexAttribArray(3);
            gl::VertexAttribPointer(
                3,
                1,
                gl::DOUBLE,
                0,
                size_vertex_data as _,
                (3 * size_of::<Vec3>()) as _,
            );

            // Albedo
            gl::EnableVertexAttribArray(4);
            gl::VertexAttribPointer(
                4,
                1,
                gl::DOUBLE,
                0,
                size_vertex_data as _,
                (3 * size_of::<Vec3>() + 1 * size_of::<Float>()) as _,
            );

            // Color Mode
            gl::EnableVertexAttribArray(5);
            gl::VertexAttribIPointer(
                5,
                1,
                gl::BYTE,
                size_vertex_data as _,
                (3 * size_of::<Vec3>() + 1 * size_of::<Float>() + 1 * size_of::<Material>()) as _,
            );

            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        }

        Self {
            vao,
            vbo,
            ebo: 0,
            len,
        }
    }

    pub fn quick_flat_vertices_buffers(vertices: &[Float], size_vertex_data: &[usize]) -> Self {
        let mut vao = 0;
        let mut vbo = 0;

        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
        }

        let number_floats_total = vertices.len();
        let number_float_per_vertex = size_vertex_data.iter().sum::<usize>();

        unsafe {
            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (number_floats_total * size_of::<Float>()) as _,
                vertices.as_ptr() as _,
                gl::STATIC_DRAW,
            );

            let mut offset = 0;
            for (index, &size) in size_vertex_data.iter().enumerate() {
                gl::EnableVertexAttribArray(index as _);
                gl::VertexAttribPointer(
                    index as _,
                    size as _,
                    gl::DOUBLE,
                    0,
                    (number_float_per_vertex * size_of::<Float>()) as _,
                    (offset * size_of::<Float>()) as _,
                );
                offset += size;
            }

            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        }

        Self {
            vao,
            vbo,
            ebo: 0,
            len: number_floats_total / number_float_per_vertex,
        }
    }

    pub fn quick_vec3_buffers(vertices: &[Vec3]) -> Self {
        let size_vertex_data = vec![3];
        let vertices_flat = vertices.iter().flat_map(|v| [v.x, v.y, v.z]).collect_vec();
        Self::quick_flat_vertices_buffers(&vertices_flat, &size_vertex_data)
    }

    pub fn update_vertex_buffer(&self, vertices: &[Vertex]) {
        let size_vertex_data = size_of::<Vertex>();

        unsafe {
            gl::BindVertexArray(self.vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);

            gl::BufferSubData(
                gl::ARRAY_BUFFER,
                0,
                (vertices.len() * size_vertex_data) as _,
                vertices.as_ptr() as _,
            );

            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        }
    }

    pub fn update_quick_vertices(&self, vertices: &[Float]) {
        let size_data_total = vertices.len() * size_of::<Float>();

        unsafe {
            gl::BindVertexArray(self.vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);

            gl::BufferSubData(
                gl::ARRAY_BUFFER,
                0,
                size_data_total as _,
                vertices.as_ptr() as _,
            );

            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        }
    }

    pub fn draw(&self, mode: GLenum) {
        unsafe {
            gl::BindVertexArray(self.vao);
        }

        if self.ebo == 0 {
            self.draw_arrays(mode);
        } else {
            self.draw_elements(mode);
        }

        unsafe {
            gl::BindVertexArray(0);
        }
    }

    pub fn draw_elements(&self, mode: GLenum) {
        unsafe {
            gl::DrawElements(mode, self.len as _, gl::UNSIGNED_INT, 0 as _);
        }
    }

    pub fn draw_arrays(&self, mode: GLenum) {
        unsafe {
            gl::DrawArrays(mode, 0, self.len as _);
        }
    }
}

#[derive(Debug)]
pub struct UBO {
    pub id: u32,
}

impl UBO {
    pub fn new() -> Self {
        Self { id: 0 }
    }
}

#[derive(Debug)]
pub struct UBOS {
    pub projection: UBO,
    pub view: UBO,
}

impl UBOS {
    pub fn new() -> Self {
        let projection = UBO::new();
        let view = UBO::new();

        Self { projection, view }
    }
}

#[derive(Debug, Clone, Copy)]
#[allow(unused)]
pub enum ProjectionParams {
    Orthographic(Float, Float, Float, Float, Float, Float),
    Perspective(),
}

#[allow(unused)]
#[derive(Debug, Clone, Copy)]
pub struct Projection {
    pub matrix: Mat4,
    params: ProjectionParams,
}

impl Projection {
    pub fn new_ortho(
        left: Float,
        right: Float,
        bottom: Float,
        top: Float,
        znear: Float,
        zfar: Float,
    ) -> Self {
        let matrix = glm::ortho(left, right, bottom, top, znear, zfar);
        let params = ProjectionParams::Orthographic(left, right, bottom, top, znear, zfar);
        Self { matrix, params }
    }

    #[allow(unused)]
    pub fn update_params(&mut self, params: ProjectionParams) {
        self.matrix = match params {
            ProjectionParams::Orthographic(left, right, bottom, top, znear, zfar) => {
                glm::ortho(left, right, bottom, top, znear, zfar)
            }
            _ => unimplemented!("projection update params"),
        };
        self.params = params;
    }

    #[allow(unused)]
    pub fn update_zfar(&mut self, zfar: Float) {
        match self.params {
            ProjectionParams::Orthographic(left, right, bottom, top, znear, _) => self
                .update_params(ProjectionParams::Orthographic(
                    left, right, bottom, top, znear, zfar,
                )),
            _ => unimplemented!("projection update params"),
        };
    }
}

#[derive(Debug)]
pub struct MapShadow {
    pub framebuffer: u32,
    pub map_depth: u32,
}

impl MapShadow {
    pub fn new(shaders: &Shaders, settings: &WindowSettings) -> Self {
        shaders.body.set_i("map_shadow", 0);
        shaders.debug_depth.set_i("map_depth", 0);

        let width_texture = settings.width_viewport_depthmap();
        let height_texture = settings.height_viewport_depthmap();

        let mut framebuffer = 0;
        let mut map_depth = 0;

        unsafe {
            gl::GenFramebuffers(1, &mut framebuffer);
            gl::BindFramebuffer(gl::FRAMEBUFFER, framebuffer);

            gl::GenTextures(1, &mut map_depth);
            gl::BindTexture(gl::TEXTURE_2D, map_depth);

            // For Depth textured framebuffer.
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::DEPTH_COMPONENT as _,
                width_texture as _,
                height_texture as _,
                0,
                gl::DEPTH_COMPONENT,
                gl::FLOAT,
                ptr::null() as _,
            );

            let color_borders: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_BORDER as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_BORDER as _);

            gl::TexParameterfv(
                gl::TEXTURE_2D,
                gl::TEXTURE_BORDER_COLOR,
                color_borders.as_ptr(),
            );

            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::DEPTH_ATTACHMENT,
                gl::TEXTURE_2D,
                map_depth,
                0,
            );

            gl::DrawBuffer(gl::NONE);
            gl::ReadBuffer(gl::NONE);

            /*
            // For Color RGB textured framebuffer.
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGB.0 as _,
                width_texture as _,
                height_texture as _,
                0,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                ptr::null() as _,
            );

            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR.0 as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR.0 as _);

            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                map_depth,
                0,
            );

            let mut rbo = 0;
            gl::GenRenderbuffers(1, &mut rbo);
            gl::BindRenderbuffer(gl::RENDERBUFFER, rbo);
            gl::RenderbufferStorage(gl::RENDERBUFFER, gl::DEPTH24_STENCIL8, width_texture as _, height_texture as _);
            gl::FramebufferRenderbuffer(gl::FRAMEBUFFER, gl::DEPTH_STENCIL_ATTACHMENT, gl::RENDERBUFFER, rbo);
            */

            if gl::CheckFramebufferStatus(gl::FRAMEBUFFER) != gl::FRAMEBUFFER_COMPLETE {
                panic!("bad framebuffer depthmap");
            }

            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        }

        Self {
            framebuffer,
            map_depth,
        }
    }

    pub fn update(&self, settings: &WindowSettings) {
        let width_texture = settings.width_viewport_depthmap();
        let height_texture = settings.height_viewport_depthmap();

        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.framebuffer);

            gl::BindTexture(gl::TEXTURE_2D, self.map_depth);

            // For Depth textured framebuffer.
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::DEPTH_COMPONENT as _,
                width_texture as _,
                height_texture as _,
                0,
                gl::DEPTH_COMPONENT,
                gl::FLOAT,
                ptr::null() as _,
            );

            let color_borders: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_BORDER as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_BORDER as _);

            gl::TexParameterfv(
                gl::TEXTURE_2D,
                gl::TEXTURE_BORDER_COLOR,
                color_borders.as_ptr(),
            );

            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::DEPTH_ATTACHMENT,
                gl::TEXTURE_2D,
                self.map_depth,
                0,
            );

            gl::DrawBuffer(gl::NONE);
            gl::ReadBuffer(gl::NONE);

            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        }
    }
}

#[derive(Debug)]
pub struct PixelInfo {
    pub object_id: u32,
    pub draw_id: u32,
    pub primitive_id: u32,
}

#[derive(Debug)]
pub struct Picker {
    pub fbo: u32,
    pub texture_picking: u32,
    pub texture_depth: u32,
    pub pick: Option<(i32, i32)>,
    pub picked_on: Option<(usize, usize)>,
}

impl Picker {
    pub fn new(settings: &WindowSettings) -> Self {
        let mut fbo = 0;
        let mut texture_picking = 0;
        let mut texture_depth = 0;

        let width = settings.width;
        let height = settings.height;

        unsafe {
            // FBO.
            gl::GenFramebuffers(1, &mut fbo);
            gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);

            // Texture object for primitive information buffer.
            gl::GenTextures(1, &mut texture_picking);
            gl::BindTexture(gl::TEXTURE_2D, texture_picking);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGB32UI as _,
                width as _,
                height as _,
                0,
                gl::RGB_INTEGER,
                gl::UNSIGNED_INT,
                ptr::null() as _,
            );
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                texture_picking,
                0,
            );

            // Texture for depth buffer.
            gl::GenTextures(1, &mut texture_depth);
            gl::BindTexture(gl::TEXTURE_2D, texture_depth);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::DEPTH_COMPONENT as _,
                width as _,
                height as _,
                0,
                gl::DEPTH_COMPONENT,
                gl::FLOAT,
                ptr::null() as _,
            );
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::DEPTH_ATTACHMENT,
                gl::TEXTURE_2D,
                texture_depth,
                0,
            );

            // Check FBO.
            let status = gl::CheckFramebufferStatus(gl::FRAMEBUFFER);
            if status != gl::FRAMEBUFFER_COMPLETE {
                panic!("bad framebuffer picker: {}", status);
            }

            // Unbind.
            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        }
        Self {
            fbo,
            texture_picking,
            texture_depth,
            pick: None,
            picked_on: None,
        }
    }

    pub fn pick(&mut self, x: i32, y: i32) {
        self.pick = Some((x, y));
    }

    pub fn use_pick_to_ndc(&mut self, settings: &WindowSettings) -> Option<(Float, Float)> {
        if let Some((x, y)) = self.pick.take() {
            // println!("{} {}", x, y);

            let ndc_x = 2.0 * x as Float / settings.width as Float - 1.0;
            let ndc_y = 2.0 * y as Float / settings.height as Float - 1.0;
            return Some((ndc_x, ndc_y));
        }
        None
    }
}

#[derive(Debug)]
pub struct GraphicalPipeline {
    pub shaders: Shaders,
    pub ubos: UBOS,
    pub mapshadow: MapShadow,
    pub vao_debug_depth: VAO,
    pub picker: RefCell<Picker>,
}

impl GraphicalPipeline {
    pub fn new(settings: &WindowSettings) -> Self {
        let shaders = Shaders::new();
        let ubos = UBOS::new();

        let mapshadow = MapShadow::new(&shaders, settings);

        let size_vertex_data_quad = vec![2, 2];
        #[rustfmt::skip]
        let vertices_quad = vec![
        // Positions   Texture
        -1.0,  1.0,    0.0, 1.0,
        -1.0, -1.0,    0.0, 0.0,
         1.0, -1.0,    1.0, 0.0,

        -1.0,  1.0,    0.0, 1.0,
         1.0, -1.0,    1.0, 0.0,
         1.0,  1.0,    1.0, 1.0,
        ];

        let vao_debug_depth =
            VAO::quick_flat_vertices_buffers(&vertices_quad, &size_vertex_data_quad);

        let picker = Picker::new(settings);

        Self {
            shaders,
            ubos,
            mapshadow,
            vao_debug_depth,
            picker: RefCell::new(picker),
        }
    }
}
